use crate::{
    transactions::{StakeQuoteTx, WitnessTx},
    vault::transactions::TransactionProvider,
};

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

    fn process_stakes(quotes: &[StakeQuoteTx], witness_txs: &[WitnessTx]) {
        for wtx in witness_txs {
            for quote in quotes {
                if wtx.quote_id == quote.id {
                    // TODO: put a balance change tx onto the side chain
                    info!("Found witness matching quote: {:?}", quote);
                    break;
                }
            }
        }
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

                SideChainProcessor::<T>::process_stakes(stake_quote_txs, witness_txs);

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
