use super::WalletAddress;
use crate::utils::clone_into_array;
use hdwallet::secp256k1::PublicKey;
use regex::Regex;
use std::{fmt::Display, str::FromStr};
use tiny_keccak::{Hasher, Keccak};

/// A structure for etherum hashes
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Hash(pub [u8; 32]);

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

/// A structure for ethereum address
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Address(pub [u8; 20]);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl FromStr for Address {
    type Err = &'static str;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        const INVALID_ADDRESS: &str = "Invalid ethereum address";
        lazy_static! {
            static ref ADDRESS_REGEX: Regex = Regex::new(r"^(0x)?[a-fA-F0-9]{40}$").unwrap();
        }

        if !ADDRESS_REGEX.is_match(string) {
            return Err(INVALID_ADDRESS);
        }

        let stripped = string.trim_start_matches("0x");
        let bytes = hex::decode(stripped).map_err(|_| INVALID_ADDRESS)?;
        Ok(Address(clone_into_array(&bytes)))
    }
}

impl Address {
    /// Get the ethereum address from a ECDSA public key
    ///
    /// # Example
    ///
    /// ```
    /// use blockswap::common::ethereum::Address;
    /// use hdwallet::secp256k1::PublicKey;
    /// use std::str::FromStr;
    ///
    /// let public_key = PublicKey::from_str("034ac1bb1bc5fd7a9b173f6a136a40e4be64841c77d7f66ead444e101e01348127").unwrap();
    /// let address = Address::from_public_key(public_key);
    ///
    /// assert_eq!(address.to_string(), "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5".to_owned());
    /// ```
    pub fn from_public_key(public_key: PublicKey) -> Self {
        let bytes: [u8; 65] = public_key.serialize_uncompressed();

        // apply a keccak_256 hash of the public key
        let mut result = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(&bytes[1..]); // Strip the first byte to get 64 bytes
        hasher.finalize(&mut result);

        // The last 20 bytes in hex is the ethereum address
        Address(clone_into_array(&result[12..]))
    }
}

impl Into<WalletAddress> for Address {
    fn into(self) -> WalletAddress {
        WalletAddress::new(&self.to_string())
    }
}

/// A structure for ethereum transactions
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Transaction {
    /// The transaction hash
    pub hash: Hash,
    /// The index of the transaction in the block
    pub index: u64,
    /// The block number of the transaction
    pub block_number: u64,
    /// The sender
    pub from: Address,
    /// The recipient (None when contract creation)
    pub to: Option<Address>,
    /// The transferred value
    pub value: u128,
}

impl Ord for Transaction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.block_number, self.index).cmp(&(other.block_number, other.index))
    }
}

impl PartialOrd for Transaction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
