use crate::{
    common::{
        coins::{Coin, PoolCoin},
        store::KeyValueStore,
    },
    side_chain::SideChainTx,
    transactions::{PoolChangeTx, StakeQuoteTx, WitnessTx},
    vault::transactions::TransactionProvider,
};

use std::convert::TryFrom;

use uuid::Uuid;

/// Component that matches witness transactions with quotes and processes them
pub struct SideChainProcessor<T, KVS>
where
    T: TransactionProvider,
    KVS: KeyValueStore,
{
    tx_provider: T,
    db: KVS,
}

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
    fn process_stake_tx(quote: &StakeQuoteTx, witness_txs: &[&WitnessTx]) -> Option<SideChainTx> {
        // TODO: put a balance change tx onto the side chain
        info!("Found witness matching quote: {:?}", quote);

        let mut loki_amount: Option<i128> = None;
        let mut other_amount: Option<i128> = None;

        for wtx in witness_txs {
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

                // For now we are only depositing LOKI
                let tx = PoolChangeTx {
                    id: Uuid::new_v4(),
                    coin,
                    depth_change: other_amount,
                    loki_depth_change: loki_amount,
                };

                Some(tx.into())
            }
            _ => None,
        }
    }

    /// Try to match witness transacitons with stake transactions and return a list of
    /// transactions that should be added to the side chain
    fn process_stakes(quotes: &[StakeQuoteTx], witness_txs: &[WitnessTx]) -> Vec<SideChainTx> {
        let mut new_txs = Vec::<SideChainTx>::default();

        for quote in quotes {
            // Find all relevant witness transactions
            let wtxs: Vec<&WitnessTx> = witness_txs
                .iter()
                .filter(|wtx| wtx.quote_id == quote.id)
                .collect();

            if !wtxs.is_empty() {
                if let Some(tx) = SideChainProcessor::<T, KVS>::process_stake_tx(quote, &wtxs) {
                    new_txs.push(tx);
                }
            }
        }

        new_txs
    }

    fn run_event_loop(mut self) {
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
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // Poll the side chain (via the transaction provider) and see if there are
        // any new witness transactions that should be processed
    }

    /// Start processor thread
    pub fn start(self) {
        std::thread::spawn(move || {
            info!("Starting the processor thread");
            self.run_event_loop();
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

        let stake_tx = create_fake_stake_quote(loki_amount.clone(), coin_amount.clone());
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount.clone().into(), Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, coin_amount.clone(), coin_type);

        let tx = Processor::process_stake_tx(&stake_tx, &[&wtx_loki, &wtx_eth]).unwrap();

        if let SideChainTx::PoolChangeTx(tx) = tx {
            assert_eq!(tx.depth_change as u128, coin_amount.to_atomic());
            assert_eq!(tx.loki_depth_change as u128, loki_amount.to_atomic());
        } else {
            panic!("Invalid tx type");
        }
    }

    #[test]
    fn partially_fulfilled_quotes_do_not_produce_new_tx() {
        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal(1.0);
        let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

        let stake_tx = create_fake_stake_quote(loki_amount.clone(), coin_amount.clone());
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount.clone().into(), Coin::LOKI);

        let tx = Processor::process_stake_tx(&stake_tx, &[&wtx_loki]);

        assert!(tx.is_none())
    }
}
