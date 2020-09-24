use crate::common::{coins::GenericCoinAmount, ethereum::Address, ethereum::Transaction};
use async_trait::async_trait;

/// Web3 client implementation
pub mod web3;

/// The results of fee estimate
#[derive(Debug)]
pub struct EstimateResult {
    /// The gas price at the time of the estimate
    pub gas_price: u128,
    /// The estimated gas limit
    pub gas_limit: u128,
}

/// The request of estimate fee
#[derive(Debug)]
pub struct EstimateRequest {
    /// The address that is sending
    pub from: Address,
    /// The address that is receiving
    pub to: Address,
    /// The amount being sent
    pub amount: GenericCoinAmount,
}

/// A trait describing an ethereum client
#[async_trait]
pub trait EthereumClient {
    /// Get the latest block number of the eth chain
    async fn get_latest_block_number(&self) -> Result<u64, String>;

    /// Get the transactions in the given block number.
    /// `None` if block doesn't exist.
    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>>;

    /// Get the estimated fee for the given transaction
    async fn get_estimated_fee(&self, tx: &EstimateRequest) -> Result<EstimateResult, String>;
}
