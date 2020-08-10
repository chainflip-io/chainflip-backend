use super::{BlockProcessor, StateProvider};
use crate::side_chain::SideChainBlock;

/// Configuration for the database
#[derive(Debug, Copy, Clone)]
pub struct Config {}

/// A database for storing and accessing local state
#[derive(Debug)]
pub struct Database {
    config: Config,
}

impl Database {
    /// Returns a database with the config given.
    pub fn new(config: Config) -> Self {
        Database { config }
    }
}

impl BlockProcessor for Database {
    fn get_last_processed_block_number(&self) -> Option<u32> {
        return None;
    }

    fn process_blocks(&self, _blocks: Vec<SideChainBlock>) -> Result<(), String> {
        todo!()
    }
}

impl StateProvider for Database {}
