use crate::{
    local_store::{self, ILocalStore, MemoryLocalStore},
    vault::config::BtcConfig,
    vault::config::EthConfig,
    vault::config::LokiConfig,
    vault::{
        config::{LokiRpcConfig, VaultConfig},
        transactions::MemoryTransactionsProvider,
    },
};
use std::sync::{Arc, Mutex};

/// Test helper for Bitcoin
pub mod btc;

/// Test helper for ethereum
pub mod ethereum;

/// Test helper for key value store
pub mod store;

/// Data used for testing
pub mod data;

/// Logging initialization
pub mod logging;

/// Utils for staking and unstaking
pub mod staking;

mod test_runner;
use chainflip_common::types::Network;
pub use test_runner::TestRunner;

/// Test ETH address
pub const TEST_ETH_ADDRESS: &str = "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5";
/// Test ETH salt
pub const TEST_ETH_SALT: [u8; 32] = [
    178, 214, 168, 126, 192, 105, 52, 255, 88, 87, 230, 120, 208, 115, 0, 13, 228, 47, 250, 223,
    163, 244, 100, 248, 233, 43, 188, 199, 188, 141, 34, 238,
];
/// Test LOKI address
pub const TEST_LOKI_ADDRESS: &str = "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY";
/// Test LOKI Payment id
pub const TEST_LOKI_PAYMENT_ID: [u8; 8] = [66, 15, 162, 155, 45, 154, 73, 245];
/// Test BTC address
pub const TEST_BTC_ADDRESS: &str = "tb1q6898gg3tkkjurdpl4cghaqgmyvs29p4x4h0552";

/// Test ROOT Key
pub const TEST_ROOT_KEY: &str = "xprv9s21ZrQH143K3sFfKzYqgjMWgvsE44f6gxaRvyo11R22u2p5qegToQaEi7e6e5mRq3f92g9yDQQtu488ggct5gUspippg678t1QTCwBRb85";

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

/// Get a transactions provider with a memory local store
pub fn get_transactions_provider() -> MemoryTransactionsProvider<MemoryLocalStore> {
    let store = MemoryLocalStore::new();
    let store = Arc::new(Mutex::new(store));
    MemoryTransactionsProvider::new(store)
}

/// Get a transactions provider with the given local store
pub fn get_transactions_provider_with_store<L: ILocalStore>(
    local_store: Arc<Mutex<L>>,
) -> MemoryTransactionsProvider<L> {
    MemoryTransactionsProvider::new(local_store)
}

/// Get a fake vault node config
pub fn get_fake_config() -> VaultConfig {
    let loki = LokiConfig {
        rpc: LokiRpcConfig {
            port: 8000
        },
        wallet_address: "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY".to_string(),
    };
    let eth = EthConfig {
        private_key: "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f".to_string(),
        provider_url: "http://localhost:8080".to_string(),
    };

    let btc = BtcConfig {
        master_root_key: TEST_ROOT_KEY.to_string(),
        rpc_port: 1000,
        rpc_user: "user".to_string(),
        rpc_password: "pass".to_string(),
    };

    VaultConfig {
        loki,
        net_type: Network::Testnet,
        eth,
        btc,
    }
}
