use crate::{
    common::{coins::Coin, store::KeyValueStore},
    side_chain::SideChainTx,
    transactions::{PoolChangeTx, StakeQuoteTx, StakeTx},
    vault::transactions::TransactionProvider,
};

use std::convert::TryFrom;

use super::transactions::memory_provider::{FulfilledTxWrapper, WitnessTxWrapper};
use uuid::Uuid;

/// Processing utils
pub mod utils;

/// Swap processing
mod swap;

/// Component that matches witness transactions with quotes and processes them
pub struct SideChainProcessor<T, KVS>
where
    T: TransactionProvider,
    KVS: KeyValueStore,
{
    tx_provider: T,
    db: KVS,
}

/// A set of transaction to be added to the side chain as a result
/// of a successful match between stake and witness transactions
struct StakeQuoteResult {
    stake_tx: StakeTx,
    pool_change: PoolChangeTx,
}

impl StakeQuoteResult {
    pub fn new(stake_tx: StakeTx, pool_change: PoolChangeTx) -> Self {
        StakeQuoteResult {
            stake_tx,
            pool_change,
        }
    }
}

/// Events emited by the processor
#[derive(Debug)]
pub enum ProcessorEvent {
    /// Block id processed (including all earlier blocks)
    BLOCK(u32),
}

type EventSender = crossbeam_channel::Sender<ProcessorEvent>;

impl<T, KVS> SideChainProcessor<T, KVS>
where
    T: TransactionProvider + Send + 'static,
    KVS: KeyValueStore + Send + 'static,
{
    /// Constructor taking a transaction provider
    pub fn new(tx_provider: T, kvs: KVS) -> Self {
        SideChainProcessor {
            tx_provider,
            db: kvs,
        }
    }

    /// Process a single stake quote with all witness transactions referencing it
    fn process_stake_quote(
        quote_info: &FulfilledTxWrapper<StakeQuoteTx>,
        witness_txs: &[&WitnessTxWrapper],
    ) -> Option<StakeQuoteResult> {
        // TODO: put a balance change tx onto the side chain
        info!("Found witness matching quote: {:?}", quote_info.inner);

        // TODO: only print this if a witness is not used:

        // For now only process unfulfilled ones:
        if quote_info.fulfilled {
            warn!("Witness matches an already fulfilled quote. Should refund?");
            return None;
        }

        let quote = &quote_info.inner;

        let mut loki_amount: Option<i128> = None;
        let mut other_amount: Option<i128> = None;

        // Indexes of used witness transaction
        let mut wtx_idxs = Vec::<Uuid>::default();

        for wtx in witness_txs {
            // We don't expect used quotes at this stage,
            // but let's double check this:
            if wtx.used {
                continue;
            }

            let wtx = &wtx.inner;

            match wtx.coin_type {
                Coin::LOKI => {
                    if loki_amount.is_some() {
                        error!("Unexpected second loki witness transaction");
                        return None;
                    }

                    let amount = match i128::try_from(wtx.amount) {
                        Ok(amount) => amount,
                        Err(err) => {
                            error!("Invalid amount in quote: {} ({})", wtx.amount, err);
                            return None;
                        }
                    };

                    wtx_idxs.push(wtx.id);
                    loki_amount = Some(amount);
                }
                coin_type @ _ => {
                    if coin_type == quote.coin_type.get_coin() {
                        if other_amount.is_some() {
                            error!("Unexpected second loki witness transaction");
                            return None;
                        }

                        let amount = match i128::try_from(wtx.amount) {
                            Ok(amount) => amount,
                            Err(err) => {
                                error!("Invalid amount in quote: {} ({})", wtx.amount, err);
                                return None;
                            }
                        };
                        wtx_idxs.push(wtx.id);
                        other_amount = Some(amount);
                    } else {
                        error!("Unexpected coin type: {}", coin_type);
                        return None;
                    }
                }
            }
        }

        if loki_amount.is_none() {
            info!("Loki is not yet provisioned in quote: {:?}", quote);
        }

        if other_amount.is_none() {
            info!(
                "{} is not yet provisioned in quote: {:?}",
                quote.coin_type.get_coin(),
                quote
            );
        }

        match (loki_amount, other_amount) {
            (Some(loki_amount), Some(other_amount)) => {
                let coin = quote.coin_type;

                let pool_change_tx = PoolChangeTx::new(coin, loki_amount, other_amount);

                let stake_tx = StakeTx {
                    id: Uuid::new_v4(),
                    pool_change_tx: pool_change_tx.id,
                    quote_tx: quote.id,
                    witness_txs: wtx_idxs,
                };

                Some(StakeQuoteResult::new(stake_tx, pool_change_tx))
            }
            _ => None,
        }
    }

    /// Try to match witness transacitons with stake transactions and return a list of
    /// transactions that should be added to the side chain
    fn process_stakes(
        quotes: &[FulfilledTxWrapper<StakeQuoteTx>],
        witness_txs: &[WitnessTxWrapper],
    ) -> Vec<SideChainTx> {
        let mut new_txs = Vec::<SideChainTx>::default();

        for quote_info in quotes {
            // Find all relevant witness transactions
            let wtxs: Vec<&WitnessTxWrapper> = witness_txs
                .iter()
                .filter(|wtx| !wtx.used && wtx.inner.quote_id == quote_info.inner.id)
                .collect();

            if !wtxs.is_empty() {
                if let Some(res) =
                    SideChainProcessor::<T, KVS>::process_stake_quote(quote_info, &wtxs)
                {
                    new_txs.reserve(new_txs.len() + 2);
                    new_txs.push(res.stake_tx.into());
                    new_txs.push(res.pool_change.into());
                }
            }
        }

        new_txs
    }

    /// Poll the side chain/tx_provider and use event_sender to
    /// notify of local events
    fn run_event_loop(mut self, event_sender: Option<EventSender>) {
        const DB_KEY: &'static str = "processor_next_block_idx";

        // TODO: We should probably distinguish between no value and other errors here:
        // The first block that's yet to be processed by us
        let mut next_block_idx = self.db.get_data(DB_KEY).unwrap_or(0);

        info!("Processor starting with next block idx: {}", next_block_idx);

        loop {
            let idx = self.tx_provider.sync();

            // Check if transaction provider made progress
            if idx > next_block_idx {
                let stake_quote_txs = self.tx_provider.get_stake_quote_txs();
                let witness_txs = self.tx_provider.get_witness_txs();

                let new_txs =
                    SideChainProcessor::<T, KVS>::process_stakes(stake_quote_txs, witness_txs);

                for tx in &new_txs {
                    info!("Adding new tx: {:?}", tx);
                }

                // TODO: make sure that things below happend atomically
                // (e.g. we don't want to send funds more than once if the
                // latest block info failed to have been updated)

                if let Err(err) = self.tx_provider.add_transactions(new_txs) {
                    error!("Error adding a pool change tx: {}", err);
                    panic!();
                };

                if let Err(err) = self.db.set_data(DB_KEY, Some(idx)) {
                    error!("Could not update latest block in db: {}", err);
                    // Not quote sure how to recover from this, so probably best to terminate
                    panic!("Database failure");
                }

                next_block_idx = idx;
                if let Some(sender) = &event_sender {
                    let _ = sender.send(ProcessorEvent::BLOCK(idx));
                    debug!("Processor processing block: {}", idx);
                }
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // Poll the side chain (via the transaction provider) and see if there are
        // any new witness transactions that should be processed
    }

    /// Start processor thread. If `event_sender` is provided,
    /// local events will be communicated through it.
    pub fn start(self, event_sender: Option<EventSender>) {
        std::thread::spawn(move || {
            info!("Starting the processor thread");
            self.run_event_loop(event_sender);
        });
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::{
        common::{
            coins::{CoinAmount, GenericCoinAmount},
            LokiAmount,
        },
        side_chain::MemorySideChain,
        utils::test_utils::{create_fake_stake_quote, create_fake_witness, store::MemoryKVS},
        vault::transactions::MemoryTransactionsProvider,
    };

    type Processor = SideChainProcessor<MemoryTransactionsProvider<MemorySideChain>, MemoryKVS>;

    #[test]
    fn fulfilled_quotes_should_produce_new_tx() {
        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal(1.0);
        let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

        let quote_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&quote_tx, loki_amount.into(), Coin::LOKI);
        let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);
        let wtx_eth = create_fake_witness(&quote_tx, coin_amount, coin_type);
        let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

        let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

        let res = Processor::process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth]).unwrap();

        assert_eq!(
            res.pool_change.depth_change as u128,
            coin_amount.to_atomic()
        );
        assert_eq!(
            res.pool_change.loki_depth_change as u128,
            loki_amount.to_atomic()
        );

        assert_eq!(res.stake_tx.pool_change_tx, res.pool_change.id);
        assert_eq!(res.stake_tx.quote_tx, quote_tx.inner.id);
        assert!(res.stake_tx.witness_txs.contains(&wtx_loki.inner.id));
        assert!(res.stake_tx.witness_txs.contains(&wtx_eth.inner.id));
    }

    #[test]
    fn partially_fulfilled_quotes_do_not_produce_new_tx() {
        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal(1.0);
        let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

        let quote_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&quote_tx, loki_amount.into(), Coin::LOKI);
        let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);

        let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

        let tx = Processor::process_stake_quote(&quote_tx, &[&wtx_loki]);

        assert!(tx.is_none())
    }

    #[test]
    fn witness_tx_cannot_be_reused() {
        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal(1.0);
        let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

        let quote_tx = create_fake_stake_quote(loki_amount, coin_amount);

        let wtx_loki = create_fake_witness(&quote_tx, loki_amount.into(), Coin::LOKI);
        // Witness has already been used before
        let wtx_loki = WitnessTxWrapper::new(wtx_loki, true);

        let wtx_eth = create_fake_witness(&quote_tx, coin_amount, coin_type);
        let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

        let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

        let tx = Processor::process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth]);

        assert!(tx.is_none())
    }

    #[test]
    fn quote_cannot_be_fulfilled_twice() {
        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal(1.0);
        let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

        let quote_tx = create_fake_stake_quote(loki_amount, coin_amount);

        let wtx_loki = create_fake_witness(&quote_tx, loki_amount.into(), Coin::LOKI);
        let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);

        let wtx_eth = create_fake_witness(&quote_tx, coin_amount, coin_type);
        let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

        // The quote has already been fulfilled
        let quote_tx = FulfilledTxWrapper::new(quote_tx, true);

        let tx = Processor::process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth]);

        assert!(tx.is_none())
    }

    #[test]
    fn check_staking_smaller_amounts() {
        let loki_amount = LokiAmount::from_decimal(1.0);

        let coin_type = Coin::ETH;
        let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

        let quote_tx = create_fake_stake_quote(
            LokiAmount::from_decimal(2.0),
            GenericCoinAmount::from_decimal(coin_type, 3.0),
        );
        let wtx_loki = create_fake_witness(&quote_tx, loki_amount.into(), Coin::LOKI);
        let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);
        let wtx_eth = create_fake_witness(&quote_tx, coin_amount, coin_type);
        let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

        let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

        let res = Processor::process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth]).unwrap();

        assert_eq!(
            res.pool_change.depth_change as u128,
            coin_amount.to_atomic()
        );
        assert_eq!(
            res.pool_change.loki_depth_change as u128,
            loki_amount.to_atomic()
        );

        assert_eq!(res.stake_tx.pool_change_tx, res.pool_change.id);
        assert_eq!(res.stake_tx.quote_tx, quote_tx.inner.id);
        assert!(res.stake_tx.witness_txs.contains(&wtx_loki.inner.id));
        assert!(res.stake_tx.witness_txs.contains(&wtx_eth.inner.id));
    }
}
