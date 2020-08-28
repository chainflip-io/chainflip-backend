use crate::transactions::CoinTx;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, time::SystemTime};

/// Definitions for various coins
pub mod coins;

/// Definitions for common API functionality
pub mod api;

/// Definitions for Ethereum
pub mod ethereum;

/// Definitions for Loki
pub mod loki;

pub use loki::{LokiPaymentId, LokiWalletAddress};

/// Key value store definitions
pub mod store;

pub use coins::Coin;

// Note: time is not reliable in a distributed environment,
// so it should probably be replaced by block_id when we
// go distributed

/// SystemTime wrapper
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Timestamp(SystemTime);

impl Timestamp {
    /// Create an instance from `SystemTime` (should we implement `From` trait instead?)
    pub fn new(ts: SystemTime) -> Self {
        Timestamp { 0: ts }
    }

    /// Create an instance from current time
    pub fn now() -> Self {
        Timestamp {
            0: SystemTime::now(),
        }
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

/// A representation of a block on some blockchain
#[derive(Debug)]
pub struct Block {
    /// Transactions that belong to this block
    pub txs: Vec<CoinTx>,
}
