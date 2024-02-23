#![cfg_attr(not(feature = "std-error"), no_std)]

#[macro_use]
mod macros;
mod utils;

pub mod consts;

pub type Amount = u128;
pub type SlotNumber = u64;

define_binary!(address, Address, crate::consts::SOLANA_ADDRESS_LEN, "A");
define_binary!(digest, Digest, crate::consts::SOLANA_DIGEST_LEN, "D");
define_binary!(signature, Signature, crate::consts::SOLANA_SIGNATURE_LEN, "S");

pub use crate::{address::Address, digest::Digest, signature::Signature};

#[cfg(feature = "pda")]
pub mod pda;

#[cfg(test)]
mod tests;
