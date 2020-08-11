use crate::side_chain::SideChainBlock;
use std::sync::{Arc, Mutex};
use vault_node::VaultNodeInterface;

mod api_server;
mod block_poller;
pub mod database;
pub mod vault_node;

pub struct Quoter {}

impl Quoter {
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
        let server = api_server::Server::new(vault_node_api.clone(), database.clone());

        poller.sync()?; // Make sure we have all the latest blocks

        // Start loops
        poller.poll();
        server.serve(port).await;
        Ok(())
    }
}

pub trait BlockProcessor {
    fn get_last_processed_block_number(&self) -> Option<u32>;
    fn process_blocks(&mut self, blocks: Vec<SideChainBlock>) -> Result<(), String>;
}

pub trait StateProvider {}
