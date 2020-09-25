use async_trait::async_trait;
use bitcoin::blockdata::transaction::Transaction;
use bitcoincore_rpc;
use std::sync::{Arc, Mutex};

pub mod btc;

#[async_trait]
pub trait BitcoinClient {
    /// Get the latest block number of the btc chain
    async fn get_latest_block_number(&self) -> Result<u64, String>;
    /// Get the transactions in the given block number.
    /// `None` if block doesn't exist.
    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>>;
}
