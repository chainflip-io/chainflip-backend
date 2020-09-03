use crate::{
    common::coins::{Coin, PoolCoin},
    side_chain::SideChainTx,
    transactions::{PoolChangeTx, StakeQuoteTx, WitnessTx},
    vault::transactions::TransactionProvider,
};

use std::convert::TryFrom;

use uuid::Uuid;

/// Component that matches witness transactions with quotes and processes them
pub struct SideChainProcessor<T>
where
    T: TransactionProvider,
{
    tx_provider: T,
}

impl<T> SideChainProcessor<T>
where
    T: TransactionProvider + Send + 'static,
{
    /// Constructor taking a transaction provider
    pub fn new(tx_provider: T) -> Self {
        SideChainProcessor { tx_provider }
    }

    fn process_stakes(quotes: &[StakeQuoteTx], witness_txs: &[WitnessTx]) -> Vec<SideChainTx> {
        let mut new_txs = Vec::<SideChainTx>::default();

        for wtx in witness_txs {
            if let Some(quote) = quotes.iter().find(|quote| quote.id == wtx.quote_id) {
                // TODO: put a balance change tx onto the side chain
                info!("Found witness matching quote: {:?}", quote);

                let coin = match PoolCoin::from(Coin::BTC) {
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
        // The first block that's yet to be processed by us
        let mut next_block_idx = 0;

        loop {
            let idx = self.tx_provider.sync();

            // Check if transaction provider made progress
            if idx > next_block_idx {
                let stake_quote_txs = self.tx_provider.get_stake_quote_txs();
                let witness_txs = self.tx_provider.get_witness_txs();

                let new_txs = SideChainProcessor::<T>::process_stakes(stake_quote_txs, witness_txs);

                for tx in &new_txs {
                    info!("Adding new tx: {:?}", tx);
                }

                // TODO: Adding to the chain creates blocks, so before
                // uncommenting the next line, I need to find a way to
                // ignore processed quotes

                // self.tx_provider.add_transactions(new_txs);

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
