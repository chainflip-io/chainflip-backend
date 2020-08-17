use crate::side_chain::SideChainBlock;
use std::sync::{Arc, Mutex};
use vault_node::VaultNodeInterface;

mod api;
mod block_poller;

use api::API;

/// The quoter database
pub mod database;

/// The vault node api consumer
pub mod vault_node;

/// Quoter
pub struct Quoter {}

impl Quoter {
    /// Run the Quoter logic.
    ///
    /// # Blocking
    ///
    /// This will block the thread it is run on.
    pub async fn run<V, D>(
        port: u16,
        vault_node_api: Arc<V>,
        database: Arc<Mutex<D>>,
    ) -> Result<(), String>
    where
        V: VaultNodeInterface + Send + Sync + 'static,
        D: BlockProcessor + StateProvider + Send + 'static,
    {
        let poller = block_poller::BlockPoller::new(vault_node_api.clone(), database.clone());
        poller.sync()?; // Make sure we have all the latest blocks

        // Start loops
        let poller_thread = std::thread::spawn(move || {
            poller.poll(std::time::Duration::from_secs(1));
        });

        API::serve(port, vault_node_api.clone(), database.clone());

        poller_thread
            .join()
            .map_err(|_| "An error occurred while polling".to_owned())?;

        Ok(())
    }
}

/// A trait for processing side chain blocks received from the vault node.
pub trait BlockProcessor {
    /// Get the block number that was last processed.
    fn get_last_processed_block_number(&self) -> Option<u32>;

    /// Process a list of blocks
    fn process_blocks(&mut self, blocks: Vec<SideChainBlock>) -> Result<(), String>;
}

/// A trait for providing quoter state
pub trait StateProvider {}
