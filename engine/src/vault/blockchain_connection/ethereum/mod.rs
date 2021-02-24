use crate::{
    common::{coins::GenericCoinAmount, ethereum::Hash, ethereum::Transaction},
    utils::bip44::KeyPair,
};
use chainflip_common::types::addresses::EthereumAddress;
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
    pub from: EthereumAddress,
    /// The address that is receiving
    pub to: EthereumAddress,
    /// The amount being sent
    pub amount: GenericCoinAmount,
}

/// The send transaction
#[derive(Debug)]
pub struct SendTransaction {
    /// The sending wallet
    pub from: KeyPair,
    /// The address that is receiving
    pub to: EthereumAddress,
    /// The amount being sent
    pub amount: GenericCoinAmount,
    /// The gas limit
    pub gas_limit: u128,
    /// The gas price
    pub gas_price: u128,
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

    /// Get the balance of the given address
    async fn get_balance(&self, address: EthereumAddress) -> Result<u128, String>;

    /// Send a transaction
    async fn send(&self, tx: &SendTransaction) -> Result<Hash, String>;
}
