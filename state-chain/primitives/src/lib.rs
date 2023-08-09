#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates.
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, RuntimeDebug,
};

use sp_std::{
	cmp::{Ord, PartialOrd},
	vec::Vec,
};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub mod chains;

pub use chains::{assets::any::Asset, ForeignChain};

/// An index to a block.
pub type BlockNumber = u32;

pub type FlipBalance = u128;

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

pub type ChannelId = u64;

pub type EgressCounter = u64;

pub type EgressId = (ForeignChain, EgressCounter);

pub type EthAmount = u128;

pub type AssetAmount = u128;

pub type BasisPoints = u16;

pub type BroadcastId = u32;

/// The type of the Id given to threshold signature requests. Note a single request may
/// result in multiple ceremonies, but only one ceremony should succeed.
pub type ThresholdSignatureRequestId = u32;

pub type PolkadotBlockNumber = u32;

// Bitcoin default fee, in sats per bytes, to be used if current fee is not available via chain
// tracking.
pub const DEFAULT_FEE_SATS_PER_KILO_BYTE: u64 = 102400;

// Approximate values calculated
pub const INPUT_UTXO_SIZE_IN_BYTES: u64 = 178;
pub const OUTPUT_UTXO_SIZE_IN_BYTES: u64 = 34;
pub const MINIMUM_BTC_TX_SIZE_IN_BYTES: u64 = 12;

pub const STABLE_ASSET: Asset = Asset::Usdc;

// Polkadot extrinsics are uniquely identified by <block number>-<extrinsic index>
// https://wiki.polkadot.network/docs/build-protocol-info
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub struct TxId {
	pub block_number: PolkadotBlockNumber,
	pub extrinsic_index: u32,
}

/// The very first epoch number
pub const GENESIS_EPOCH: u32 = 1;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Roles in the Chainflip network.
///
/// Chainflip's network is permissioned and only accessible to owners of accounts funded with a Flip
/// balance. In addition to being funded, the account owner is required to indicate the role they
/// intend to play in the network. This will determine in which ways the account can interact with
/// the chain.
///
/// Each account can only be associated with a single role, and the role can only be updated from
/// the initial [AccountRole::None] state.
#[derive(
	PartialEq,
	Eq,
	Clone,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	RuntimeDebug,
	Copy,
	Default,
	PartialOrd,
	Ord,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum AccountRole {
	/// The default account type - indicates a bare account with no special role or permissions.
	#[default]
	None,
	/// Validators are responsible for the maintenance and operation of the Chainflip network. This
	/// role is required for any node that wishes to participate in auctions.
	Validator,
	/// Liquidity providers can deposit assets and deploy them in trading pools.
	LiquidityProvider,
	/// Brokers submit swap deposit requests on behalf of users.
	Broker,
}

pub type EgressBatch<Amount, EgressAddress> = Vec<(Amount, EgressAddress)>;

/// Struct that represents the estimated output of a Swap.
#[derive(PartialEq, Default, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct SwapOutput {
	// Intermediary amount, if there's any
	pub intermediary: Option<AssetAmount>,
	// Final output of the swap
	pub output: AssetAmount,
}

impl From<AssetAmount> for SwapOutput {
	fn from(value: AssetAmount) -> Self {
		Self { intermediary: None, output: value }
	}
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo)]
pub enum SwapLeg {
	FromStable,
	ToStable,
}

pub type TransactionHash = [u8; 32];

#[derive(
	Copy,
	Clone,
	Debug,
	Default,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct SemVer {
	pub major: u8,
	pub minor: u8,
	pub patch: u8,
}

/// The network environment, used to determine which chains the Chainflip network is connected to.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum NetworkEnvironment {
	/// Chainflip is connected to public mainnet chains.
	Mainnet,
	/// Chainflip is connected to public testnet chains.
	Testnet,
	/// Chainflip is connected to a local development chains.
	#[default]
	Development,
}

#[cfg(feature = "std")]
impl core::fmt::Display for NetworkEnvironment {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			NetworkEnvironment::Mainnet => write!(f, "Mainnet"),
			NetworkEnvironment::Testnet => write!(f, "Testnet"),
			NetworkEnvironment::Development => write!(f, "Development"),
		}
	}
}
