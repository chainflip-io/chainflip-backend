use ring::signature::{EcdsaKeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, hash::Hash, time::SystemTime};

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

/// Liquidity provider
pub mod liquidity_provider;

pub use liquidity_provider::{Liquidity, LiquidityProvider};

pub use coins::{Coin, GenericCoinAmount, PoolCoin};

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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Hash)]
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

/// Staker's identity (Hex-encoded ECDSA P-256 Public Key)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct StakerId(String);

impl std::fmt::Display for StakerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl StakerId {
    /// Create from hex representation of the public key
    pub fn new<T: ToString>(pubkey_hex: T) -> Result<Self, &'static str> {
        let pubkey: String = pubkey_hex.to_string();

        let len = "0433829aa2cccda485ee215421bd6c2af3e6e1702e3202790af42a7332c3fc06ec08beafef0b504ed20d5176f6323da3a4d34c5761a82487087d93ebd673ca7293".len();

        dbg!(len);

        if pubkey.len() == len {
            Ok(StakerId(pubkey))
        } else {
            Err("Unexpected pubkey length")
        }
    }

    /// Get the inner representation (as hex string)
    pub fn inner(&self) -> &str {
        &self.0
    }
}

/// Staker capable of siging unstake transactions
pub struct Staker {
    /// Keypair used for signing
    pub keys: EcdsaKeyPair,
}

impl Staker {
    /// Convenience method to get hex-encoded pubkey
    pub fn public_key(&self) -> String {
        hex::encode(self.keys.public_key())
    }

    /// Convenience method to generate staker id from keys
    pub fn id(&self) -> StakerId {
        let pk = self.public_key();
        StakerId::new(pk).expect("Valid keypair shouldn't generate invalid staker id")
    }
}
