use crate::{
    common::{Coin, Timestamp, WalletAddress},
    transactions::QuoteTx,
};
use uuid::Uuid;

/// Test helpers for Block Processor
pub mod block_processor;
/// Test helpers for Vault Node API
pub mod vault_node_api;

/// Test helper for transaction provider
pub mod transaction_provider;

/// Test helper for ethereum
pub mod ethereum;

/// Test helper for key value store
pub mod store;

/// Create a dummy quote transaction to be used for tests
pub fn create_fake_quote_tx() -> QuoteTx {
    let return_address = Some(WalletAddress::new("Alice"));
    let input_address = WalletAddress::new("Bob");
    let timestamp = Timestamp::now();

    let quote = QuoteTx {
        id: Uuid::new_v4(),
        timestamp,
        input: Coin::LOKI,
        output: Coin::BTC,
        input_address_id: "".to_owned(),
        input_address,
        return_address,
        input_amount: 0,
        slippage_limit: 0.1,
    };

    quote
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
