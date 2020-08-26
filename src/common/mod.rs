use crate::transactions::CoinTx;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, time::SystemTime};

/// Definitions for various coins
pub mod coins;

/// Definitions for common API functionality
pub mod api;

/// Definitions for Ethereum
pub mod ethereum;

// Note: time is not reliable in a distributed environment,
// so it should probably be replaced by block_id when we
// go distributed

/// SystemTime wrapper
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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

/// Represents regular and integrated wallet addresses for Loki
#[derive(Debug)]
pub struct LokiWalletAddress {
    /// base58 (monero flavor) representation
    address: String,
}

impl std::str::FromStr for LokiWalletAddress {
    type Err = String;

    /// Construct from string, validating address length
    fn from_str(addr: &str) -> Result<Self, Self::Err> {
        match addr.len() {
            97 | 108 => Ok(LokiWalletAddress {
                address: addr.to_owned(),
            }),
            x @ _ => Err(format!("Invalid address length: {}", x)),
        }
    }
}

impl LokiWalletAddress {
    /// Get internal string representation
    pub fn to_str(&self) -> &str {
        &self.address
    }
}
/// Payment id that can be used to identify loki transactions
#[derive(Debug)]
pub struct LokiPaymentId {
    // String representation of payment id with trailing zeros added at construction time.
    // We might consider using a constant size array/string on the stack for this
    long_pid: String,
}

impl LokiPaymentId {
    /// Get payment id as a string slice
    pub fn to_str(&self) -> &str {
        &self.long_pid
    }
}

impl std::str::FromStr for LokiPaymentId {
    type Err = String;

    /// Construct loki payment id from a short (long) 16 (64) hex character string
    fn from_str(pid: &str) -> Result<Self, String> {
        match pid.len() {
            16 => {
                // There is a bug in the wallet that requires trailing zero. Apparently
                // there is a fix for that on some branch, but for now let's just
                // defencively add zeros on our side
                let long_pid = format!("{}000000000000000000000000000000000000000000000000", pid);

                Ok(LokiPaymentId { long_pid })
            }
            64 => Ok(LokiPaymentId {
                long_pid: pid.to_owned(),
            }),
            x @ _ => Err(format!("Incorect size for short payment id: {}", x)),
        }
    }
}

/// Serialize LokiPaymentId as a simple string
impl serde::Serialize for LokiPaymentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.long_pid)
    }
}

/// A representation of a block on some blockchain
#[derive(Debug)]
pub struct Block {
    /// Transactions that belong to this block
    pub txs: Vec<CoinTx>,
}
