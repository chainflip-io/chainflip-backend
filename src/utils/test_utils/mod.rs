use crate::{
    common::{Coin, Timestamp, WalletAddress},
    side_chain::{ISideChain, MemorySideChain},
    transactions::OutputTx,
    transactions::QuoteTx,
    vault::transactions::MemoryTransactionsProvider,
};
use std::sync::{Arc, Mutex};

/// Test helper for Bitcoin
pub mod btc;

/// Test helper for ethereum
pub mod ethereum;

/// Test helper for key value store
pub mod store;

/// Transactions used for testing
pub mod fake_txs;

/// Logging initialization
pub mod logging;

pub use fake_txs::{create_fake_stake_quote, create_fake_witness};

/// Test ETH address
pub const TEST_ETH_ADDRESS: &str = "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5";
/// Test LOKI address
pub const TEST_LOKI_ADDRESS: &str = "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY";
/// Test BTC address
pub const TEST_BTC_ADDRESS: &str = "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy";

/// Create a dummy quote transaction to be used for tests
pub fn create_fake_quote_tx_eth_loki() -> QuoteTx {
    let eth_address = WalletAddress::new(TEST_ETH_ADDRESS);

    create_fake_quote_tx_coin_to_loki(Coin::ETH, eth_address)
}

/// Creates a fake quote tx from any input coin and address, to Loki
pub fn create_fake_quote_tx_coin_to_loki(i_coin: Coin, i_addr: WalletAddress) -> QuoteTx {
    let loki_address = WalletAddress::new(TEST_LOKI_ADDRESS);

    create_fake_quote_tx(i_coin, i_addr, Coin::LOKI, loki_address)
}

/// Create dummy quote tx generalised
pub fn create_fake_quote_tx(
    i_coin: Coin,
    i_addr: WalletAddress,
    o_coin: Coin,
    o_addr: WalletAddress,
) -> QuoteTx {
    QuoteTx::new(
        Timestamp::now(),
        i_coin,
        i_addr.clone(),
        "7".to_string(),
        Some(i_addr),
        o_coin,
        o_addr,
        1.0,
        0.0,
    )
    .expect("Expected valid quote")
}

/// Create a fake output tx
pub fn create_fake_output_tx(coin: Coin) -> OutputTx {
    let address= match coin {
        Coin::LOKI => "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY",
        Coin::ETH => "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5",
        // p2pkh testnet address
        Coin::BTC => "msXs47UUTTMr4Kqe2WaDfZCR9t9qb25WJo",
        _ => "Address"
    };

    OutputTx {
        id: uuid::Uuid::new_v4(),
        timestamp: Timestamp::now(),
        quote_tx: uuid::Uuid::new_v4(),
        witness_txs: vec![],
        pool_change_txs: vec![],
        coin,
        address: WalletAddress::new(address),
        amount: 100,
    }
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
