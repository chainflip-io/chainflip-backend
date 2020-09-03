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

    fn process_stakes(quotes: &[StakeQuoteTx], witness_txs: &[WitnessTx]) -> Vec<SideChainTx> {
        let mut new_txs = Vec::<SideChainTx>::default();

        for wtx in witness_txs {
            if let Some(quote) = quotes.iter().find(|quote| quote.id == wtx.quote_id) {
                // TODO: put a balance change tx onto the side chain
                info!("Found witness matching quote: {:?}", quote);

                let coin = match PoolCoin::from(Coin::ETH) {
                    Ok(coin) => coin,
                    Err(err) => {
                        error!("Invalid quote ({})", err);
                        continue;
                    }
                };

                let loki_depth_change = match i128::try_from(wtx.amount) {
                    Ok(amount) => amount,
                    Err(err) => {
                        error!("Invalid amount in quote: {} ({})", wtx.amount, err);
                        continue;
                    }
                };

                // For now we are only depositing LOKI
                let pool_tx = PoolChangeTx {
                    id: Uuid::new_v4(),
                    coin,
                    depth_change: 0,
                    loki_depth_change,
                };

                new_txs.push(pool_tx.into());
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
