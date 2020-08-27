use crate::common::ethereum::Transaction;
use async_trait::async_trait;

/// Web3 client implementation
pub mod web3;

/// A trait describing an ethereum client
#[async_trait]
pub trait EthereumClient {
    /// Get the latest block number of the eth chain
    async fn get_latest_block_number(&self) -> Result<u64, String>;
    /// Get the transactions in the given block number.
    /// `None` if block doesn't exist.
    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>>;
}
