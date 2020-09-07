use crate::transactions::CoinTx;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, hash::Hash, time::SystemTime};

/// The Loki processing fee
pub static LOKI_PROCESS_FEE_DECIMAL: f64 = 0.5;

/// Definitions for various coins
pub mod coins;

/// Definitions for common API functionality
pub mod api;

/// Definitions for Ethereum
pub mod ethereum;

/// Definitions for Loki
pub mod loki;

pub use loki::{LokiAmount, LokiPaymentId, LokiWalletAddress};

/// Key value store definitions
pub mod store;

pub use coins::Coin;

// Note: time is not reliable in a distributed environment,
// so it should probably be replaced by block_id when we
// go distributed

/// Unix millisecond timestamp wrapper
#[derive(Debug, Copy, Clone, Ord, PartialOrd, PartialEq, Eq, Deserialize, Serialize)]
pub struct Timestamp(pub u128);

impl Timestamp {
    /// Create an instance from `SystemTime`
    pub fn from_system_time(ts: SystemTime) -> Self {
        let millis = ts
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Failed to get unix timestamp")
            .as_millis();
        Timestamp(millis)
    }

    /// Create an instance from current system time
    pub fn now() -> Self {
        Timestamp::from_system_time(SystemTime::now())
    }
}

/// A wrapper around String to be used as wallet address.
/// We might want to use separate type for each type of
/// wallet/blockchain
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WalletAddress(pub String);

impl Display for WalletAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl WalletAddress {
    /// Create address from string
    pub fn new(address: &str) -> Self {
        WalletAddress {
            0: address.to_owned(),
        }
    }
}

impl Hash for WalletAddress {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

/// A representation of a block on some blockchain
#[derive(Debug)]
pub struct Block {
    /// Transactions that belong to this block
    pub txs: Vec<CoinTx>,
}
