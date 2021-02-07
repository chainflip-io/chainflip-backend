use std::sync::Arc;

use crate::{common::store::KeyValueStore, vault::transactions::TransactionProvider};

use chainflip_common::types::Network;
pub use output::{
    BtcOutputSender, CoinProcessor, EthOutputSender, OutputCoinProcessor, OxenSender,
};
use parking_lot::RwLock;

/// Processing utils
pub mod utils;

/// Swap processing
mod swap;

/// Output processing
mod output;

/// deposit and withdraw processing
mod staking;

/// Component that matches witnesses with quotes and processes them
pub struct SideChainProcessor<T, KVS, S>
where
    T: TransactionProvider,
    KVS: KeyValueStore,
    S: CoinProcessor,
{
    tx_provider: Arc<RwLock<T>>,
    db: KVS,
    coin_sender: S,
    network: Network,
}

/// Events emited by the processor
#[derive(Debug)]
pub enum ProcessorEvent {
    /// Last event processed (including all earlier events)
    EVENT(u64),
}

type EventSender = crossbeam_channel::Sender<ProcessorEvent>;

// TODO: STate chain processor?
impl<T, KVS, S> SideChainProcessor<T, KVS, S>
where
    T: TransactionProvider + Send + Sync + 'static,
    KVS: KeyValueStore + Send + 'static,
    S: CoinProcessor + Send + 'static,
{
    /// Constructor taking a transaction provider
    pub fn new(tx_provider: Arc<RwLock<T>>, kvs: KVS, coin_sender: S, network: Network) -> Self {
        SideChainProcessor {
            tx_provider,
            db: kvs,
            coin_sender,
            network,
        }
    }

    async fn on_blockchain_progress(&mut self) {
        println!("On blockchain progress");
        staking::process_deposit_quotes(&mut self.tx_provider, self.network);

        staking::process_withdraw_requests(&mut *self.tx_provider.write(), self.network);

        swap::process_swaps(&mut self.tx_provider, self.network);

        output::process_outputs(&mut self.tx_provider, &mut self.coin_sender).await;
    }

    /// Poll the side chain/tx_provider and use event_sender to
    /// notify of local events
    async fn run_event_loop(mut self, event_sender: Option<EventSender>) {
        const DB_KEY: &'static str = "processor_next_event";

        // TODO: We should probably distinguish between no value and other errors here:
        // The first block that's yet to be processed by us
        let mut next_event: u64 = self.db.get_data(DB_KEY).unwrap_or(0);

        info!("Processor starting with next event number: {}", next_event);

        loop {
            let curr_event = self.tx_provider.write().sync();

            if curr_event > next_event {
                debug!("Provider is at block: {}", curr_event);
            }

            // Check if transaction provider made progress
            if curr_event >= next_event {
                self.on_blockchain_progress().await;
            }

            if let Err(err) = self.db.set_data(DB_KEY, Some(curr_event)) {
                error!("Could not update latest event in db: {}", err);
                // Not quote sure how to recover from this, so probably best to terminate
                panic!("Database failure");
            }

            next_event = curr_event;
            if let Some(sender) = &event_sender {
                let _ = sender.send(ProcessorEvent::EVENT(curr_event));
                debug!("Processor processing event at {}", curr_event);
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // Poll the side chain (via the transaction provider) and see if there are
        // any new witnesses that should be processed
    }

    /// Start processor thread. If `event_sender` is provided,
    /// local events will be communicated through it.
    pub fn start(self, event_sender: Option<EventSender>) {
        std::thread::spawn(move || {
            info!("Starting the processor thread");

            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                self.run_event_loop(event_sender).await;
            });
        });
    }
}
