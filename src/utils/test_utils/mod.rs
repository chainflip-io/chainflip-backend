use crate::{
    common::{Coin, Timestamp, WalletAddress},
    side_chain::{ISideChain, MemorySideChain},
    transactions::QuoteTx,
    vault::transactions::MemoryTransactionsProvider,
};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Test helpers for Block Processor
pub mod block_processor;
/// Test helpers for Vault Node API
pub mod vault_node_api;

/// Test helper for ethereum
pub mod ethereum;

/// Test helper for key value store
pub mod store;

/// Transactions used for testing
pub mod fake_txs;

/// Logging initialization
pub mod logging;

pub use fake_txs::{create_fake_stake_quote, create_fake_unstake_request_tx, create_fake_witness};

/// Create a dummy quote transaction to be used for tests
pub fn create_fake_quote_tx() -> QuoteTx {
    let eth_address = WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5");
    let loki_address = WalletAddress::new("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY");

    QuoteTx::new(
        Timestamp::now(),
        Coin::ETH,
        eth_address.clone(),
        "7".to_string(),
        Some(eth_address),
        Coin::LOKI,
        loki_address,
        1.0,
        0.0,
    )
    .expect("Expected valid quote")
}

/// Creates a new random file name that (if created)
/// gets removed when this object is destructed
pub struct TempRandomFile {
    path: String,
}

impl TempRandomFile {
    /// Creates a random file name
    pub fn new() -> Self {
        use rand::Rng;

        let rand_filename = format!("temp-{}.db", rand::thread_rng().gen::<u64>());

        TempRandomFile {
            path: rand_filename,
        }
    }

    /// Get the internal file name
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Drop for TempRandomFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path)
            .expect(&format!("Could not remove temp file {}", &self.path));
    }
}

/// Get a transactions provider with a memory side chain
pub fn get_transactions_provider() -> MemoryTransactionsProvider<MemorySideChain> {
    let chain = MemorySideChain::new();
    let chain = Arc::new(Mutex::new(chain));
    MemoryTransactionsProvider::new(chain)
}

/// Get a transactions provider with the given side chain
pub fn get_transactions_provider_with_chain<S: ISideChain>(
    side_chain: Arc<Mutex<S>>,
) -> MemoryTransactionsProvider<S> {
    MemoryTransactionsProvider::new(side_chain)
}
