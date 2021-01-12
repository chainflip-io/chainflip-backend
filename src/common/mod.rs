use ring::signature::{EcdsaKeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, fmt::Display, hash::Hash};

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

pub use coins::{GenericCoinAmount, PoolCoin};

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

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        let string = hex::encode(bytes);
        Self::new(string)
    }

    /// Get the inner representation (as hex string)
    pub fn inner(&self) -> &str {
        &self.0
    }

    /// Get the byte representation of the staker id
    pub fn bytes(&self) -> [u8; 65] {
        hex::decode(self.0.clone()).unwrap().try_into().unwrap()
    }
}

impl<T: AsRef<[u8]>> PartialEq<T> for StakerId {
    fn eq(&self, other: &T) -> bool {
        self.bytes() == other.as_ref()
    }
}

impl PartialEq<StakerId> for Vec<u8> {
    fn eq(&self, other: &StakerId) -> bool {
        *self == &other.bytes()
    }
}

/// Staker capable of siging withdraw transactions
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
