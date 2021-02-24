use crate::{
    common::{GenericCoinAmount, WalletAddress},
    utils::bip44::KeyPair,
};
use bitcoin::{Address, Txid};
use spv::AddressUnspentResponse;

/// Define btc SPV interface
pub mod spv;

/// Everything required to send a bitcoin transaction
#[derive(Debug)]
pub struct SendTransaction {
    /// The address that is sending
    pub from: KeyPair,
    /// The address that is receiving
    pub to: Address,
    /// The amount being sent
    pub amount: GenericCoinAmount,
}

/// Required trait for implementor to be able to send BTC
#[async_trait]
pub trait IBitcoinSend {
    /// Creates and sends a bitcoin transaction on the bitcoin network, returns the BTC txid
    async fn send(&self, tx: &SendTransaction) -> Result<Txid, String>;

    /// Gets the balance of any address on the bitcoin network
    async fn get_address_balance(
        &self,
        address: WalletAddress,
    ) -> Result<GenericCoinAmount, String>;
}

#[async_trait]
/// Defines the interface for a bitcoin SPV client, used by a bitcoin SPV witness
pub trait BitcoinSPVClient {
    /// Estimate the fee of a Segwit tx
    async fn get_estimated_fee(
        &self,
        send_tx: &SendTransaction,
        fee_method: spv::FeeMethod,
        fee_level: u32,
    ) -> Result<u64, String>;

    /// Returns UTXO list of any address
    async fn get_address_unspent(
        &self,
        address: &WalletAddress,
    ) -> Result<AddressUnspentResponse, String>;
}
