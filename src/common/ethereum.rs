use crate::utils::clone_into_array;
use chainflip_common::types::addresses::EthereumAddress;
use regex::Regex;
use std::{fmt::Display, str::FromStr};

/// A structure for etherum hashes
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Hash(pub [u8; 32]);

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl FromStr for Hash {
    type Err = &'static str;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        const INVALID_HASH: &str = "Invalid ethereum hash";
        lazy_static! {
            static ref HASH_REGEX: Regex = Regex::new(r"^(0x)?[a-fA-F0-9]{64}$").unwrap();
        }

        if !HASH_REGEX.is_match(string) {
            return Err(INVALID_HASH);
        }

        let stripped = string.trim_start_matches("0x").to_lowercase();
        let bytes = hex::decode(stripped).map_err(|_| INVALID_HASH)?;
        Ok(Hash(clone_into_array(&bytes)))
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
    pub from: EthereumAddress,
    /// The recipient (None when contract creation)
    pub to: Option<EthereumAddress>,
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
