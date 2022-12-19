#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{crypto::AccountId32, H160};
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

/// An index to a block.
pub type BlockNumber = u32;

pub type FlipBalance = u128;

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

pub type IntentId = u64;

pub type EgressCounter = u64;

pub type EgressId = (ForeignChain, EgressCounter);

pub type ExchangeRate = FixedU128;

pub type EthereumAddress = [u8; 20];

pub type EthAmount = u128;

pub type AssetAmount = u128;

pub type BasisPoints = u16;

pub type BroadcastId = u32;

/// Alias to the opaque account ID type for this chain, actually a `AccountId32`. This is always
/// 32 bytes.
pub type PolkadotAccountId = AccountId32;

pub type PolkadotBlockNumber = u32;

// Polkadot extrinsics are uniquely identified by <block number>-<extrinsic index>
// https://wiki.polkadot.network/docs/build-protocol-info
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub struct TxId {
	pub block_number: PolkadotBlockNumber,
	pub extrinsic_index: u32,
}

pub const ETHEREUM_ETH_ADDRESS: EthereumAddress = [0xEE; 20];

/// The very first epoch number
pub const GENESIS_EPOCH: u32 = 1;

pub type KeyId = Vec<u8>;

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

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy, PartialOrd, Ord,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(EthereumAddress),
	Dot([u8; 32]),
}

#[cfg(feature = "std")]
impl core::fmt::Display for ForeignChainAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			ForeignChainAddress::Eth(addr) => {
				write!(f, "Eth(0x{})", hex::encode(addr))
			},
			ForeignChainAddress::Dot(addr) => {
				write!(f, "Dot(0x{})", hex::encode(addr))
			},
		}
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
	InvalidAddress,
}

impl AsRef<[u8]> for ForeignChainAddress {
	fn as_ref(&self) -> &[u8] {
		match self {
			ForeignChainAddress::Eth(address) => address.as_slice(),
			ForeignChainAddress::Dot(address) => address.as_slice(),
		}
	}
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

impl TryFrom<ForeignChainAddress> for PolkadotAccountId {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Dot(addr) => Ok(addr.into()),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

// For MockEthereum
impl TryFrom<ForeignChainAddress> for u64 {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr[0] as u64),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}
impl From<u64> for ForeignChainAddress {
	fn from(address: u64) -> ForeignChainAddress {
		ForeignChainAddress::Eth([address as u8; 20])
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

impl From<PolkadotAccountId> for ForeignChainAddress {
	fn from(address: PolkadotAccountId) -> ForeignChainAddress {
		ForeignChainAddress::Dot(address.into())
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
