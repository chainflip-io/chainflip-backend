#![cfg_attr(not(feature = "std-error"), no_std)]

pub use crate::{address::Address, digest::Digest, signature::Signature};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[macro_use]
mod macros;

#[cfg(feature = "pda")]
pub mod pda;

#[cfg(test)]
mod tests;

pub mod consts;
mod utils;

pub type Amount = u64;
pub type SlotNumber = u64;
pub type ComputeLimit = u32;
pub type AccountBump = u8;

define_binary!(address, Address, crate::consts::SOLANA_ADDRESS_LEN, "A");
define_binary!(digest, Digest, crate::consts::SOLANA_DIGEST_LEN, "D");
define_binary!(signature, Signature, crate::consts::SOLANA_SIGNATURE_LEN, "S");

/// Represents a derived Associated Token Account to be used as deposit channels.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct DerivedAta {
	pub address: Address,
	pub bump: AccountBump,
}
