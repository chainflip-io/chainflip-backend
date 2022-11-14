#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H160;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	FixedU128, MultiSignature, RuntimeDebug,
};

use sp_std::vec::Vec;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub mod chains;

pub use chains::{assets::any::Asset, ForeignChain};
pub mod liquidity;
pub use liquidity::*;

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

pub type IntentId = u64;

pub type ExchangeRate = FixedU128;

pub type EthereumAddress = [u8; 20];

pub type EthAmount = u128;

pub type AssetAmount = u128;

pub const ETHEREUM_ETH_ADDRESS: EthereumAddress = [0xEE; 20];

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Roles in the Chainflip network.
///
/// Chainflip's network is permissioned and only accessible to owners of accounts with staked Flip.
/// In addition to staking, the account owner is required to indicate the role they intend to play
/// in the network. This will determine in which ways the account can interact with the chain.
///
/// Each account can only be associated with a single role, and the role can only be updated from
/// the initial [AccountRole::None] state.
#[derive(PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum AccountRole {
	/// The default account type - indicates a bare account with no special role or permissions.
	None,
	/// Validators are responsible for the maintenance and operation of the Chainflip network. This
	/// role is required for any node that wishes to participate in auctions.
	Validator,
	/// Liquidity providers can deposit assets and deploy them in trading pools.
	LiquidityProvider,
	/// Relayers submit swap intents on behalf of users.
	Relayer,
}

impl Default for AccountRole {
	fn default() -> Self {
		AccountRole::None
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(EthereumAddress),
	Dot([u8; 32]),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
	InvalidAddress,
}

impl TryFrom<ForeignChainAddress> for EthereumAddress {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for H160 {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr.into()),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for [u8; 32] {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Dot(addr) => Ok(addr),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl From<EthereumAddress> for ForeignChainAddress {
	fn from(address: EthereumAddress) -> ForeignChainAddress {
		ForeignChainAddress::Eth(address)
	}
}

impl From<H160> for ForeignChainAddress {
	fn from(address: H160) -> ForeignChainAddress {
		ForeignChainAddress::Eth(address.to_fixed_bytes())
	}
}

impl From<[u8; 32]> for ForeignChainAddress {
	fn from(address: [u8; 32]) -> ForeignChainAddress {
		ForeignChainAddress::Dot(address)
	}
}

impl From<ForeignChainAddress> for ForeignChain {
	fn from(address: ForeignChainAddress) -> ForeignChain {
		match address {
			ForeignChainAddress::Eth(_) => ForeignChain::Ethereum,
			ForeignChainAddress::Dot(_) => ForeignChain::Polkadot,
		}
	}
}

pub type EgressBatch<Amount, EgressAddress> = Vec<(Amount, EgressAddress)>;
