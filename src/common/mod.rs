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

impl std::str::FromStr for Timestamp {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ts: u128 = s.parse().map_err(|_| "Timestamp must be valid u128")?;

        Ok(Timestamp(ts))
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

        /// Expected pubkey length in hex (65 bytes)
        const PUBKEY_LEN: usize = 130;

        if pubkey.len() == PUBKEY_LEN {
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

/// Fraction of the total owned amount to unstake
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub struct UnstakeFraction(u32);

impl UnstakeFraction {
    /// Value representing 100% ownership
    pub const MAX: UnstakeFraction = UnstakeFraction(10_000);

    /// Create an instance if valid
    pub fn new(fraction: u32) -> Result<Self, &'static str> {
        if fraction < 1 || fraction > UnstakeFraction::MAX.0 {
            Err("Fraction must be in the range (0; 10_000]")
        } else {
            Ok(UnstakeFraction(fraction))
        }
    }
}

impl std::str::FromStr for UnstakeFraction {
    type Err = &'static str;

    fn from_str(f: &str) -> Result<Self, Self::Err> {
        let fraction: u32 = f.parse().map_err(|_| "fraction must be an integer")?;

        UnstakeFraction::new(fraction)
    }
}

impl Display for UnstakeFraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
