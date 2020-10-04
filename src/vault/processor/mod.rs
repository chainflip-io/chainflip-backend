use crate::{common::store::KeyValueStore, vault::transactions::TransactionProvider};

/// Processing utils
pub mod utils;

/// Swap processing
mod swap;

/// Output processing
mod output;

/// Stake and unstake processing
mod staking;

/// Component that matches witness transactions with quotes and processes them
pub struct SideChainProcessor<T, KVS>
where
    T: TransactionProvider,
    KVS: KeyValueStore,
{
    tx_provider: T,
    db: KVS,
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

    fn on_blockchain_progress(&mut self) {
        let stake_quote_txs = self.tx_provider.get_stake_quote_txs();
        let witness_txs = self.tx_provider.get_witness_txs();

        let new_txs = staking::process_stakes(stake_quote_txs, witness_txs);

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
                self.on_blockchain_progress();
            }

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
