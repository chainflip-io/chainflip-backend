use async_trait::async_trait;
use bitcoin::blockdata::transaction::Transaction;
use bitcoin::Network;

/// Define btc interface
pub mod btc;

#[async_trait]
/// Defines the interface for a bitcoin client, used by a bitcoin witness
pub trait BitcoinClient {
    /// Get the latest block number of the btc chain
    async fn get_latest_block_number(&self) -> Result<u64, String>;
    /// Get the transactions in the given block number.
    /// `None` if block doesn't exist.
    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>>;

    /// Get network type of the btc client (bitcoin, testnet, regtest)
    fn get_network_type(&self) -> Network;
}
