use crate::side_chain::SideChainBlock;
use crate::transactions::QuoteTx;

/// Configuration for the vault node api
#[derive(Debug, Copy, Clone)]
pub struct Config {}

/// An interface for interacting with the vault node.
pub trait VaultNodeInterface {
    fn get_blocks(&self, start: u32, limit: u32) -> Result<Vec<SideChainBlock>, String>;
    fn submit_quote(&self) -> Result<QuoteTx, String>; // TODO: Change Result type to a QuoteResponse?
}

/// A
#[derive(Debug)]
pub struct VaultNodeAPI {
    config: Config,
}

impl VaultNodeAPI {
    /// Returns the vault node api with the config given.
    pub fn new(config: Config) -> Self {
        VaultNodeAPI { config }
    }
}

impl VaultNodeInterface for VaultNodeAPI {
    fn get_blocks(&self, _start: u32, _limit: u32) -> Result<Vec<SideChainBlock>, String> {
        todo!()
    }

    fn submit_quote(&self) -> Result<QuoteTx, String> {
        todo!()
    }
}
