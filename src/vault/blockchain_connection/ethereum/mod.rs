use crate::common::ethereum::Transaction;
use async_trait::async_trait;

/// A trait describing an ethereum client
#[async_trait]
pub trait EthereumClient {
    /// Get the latest block number of the eth chain
    async fn get_latest_block_number(&self) -> u64;
    /// Get the transactions in the given block number.
    /// `None` if block doesn't exist.
    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>>;
}
