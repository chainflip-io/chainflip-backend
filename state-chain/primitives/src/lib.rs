#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates.
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, Percent, RuntimeDebug,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	cmp::{Ord, PartialOrd},
	vec::Vec,
};

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

pub type Ed25519PublicKey = sp_core::ed25519::Public;
pub type Ipv6Addr = u128;
pub type Port = u16;

pub const FLIP_DECIMALS: u32 = 18;
pub const FLIPPERINOS_PER_FLIP: FlipBalance = 10u128.pow(FLIP_DECIMALS);

// Bitcoin default fee, in sats per bytes, to be used if current fee is not available via chain
// tracking.
pub const DEFAULT_FEE_SATS_PER_KILOBYTE: u64 = 102400;

// To spend one of our deposit UTXOs, we need:
// 32 bytes for the TX ID
// 4 bytes for the output index
// 1 byte to indicate that this is a segwit input (i.e. unlock script length = 0)
// 4 bytes for the sequence number
// In addition to that we need the witness data, which is:
// 1 byte for the number of witness elements
// 65 bytes for the signature
// 1 length byte for the unlock script
// up to 9 bytes for the salt
// 1 byte to drop the salt
// 33 bytes for the pubkey
// 1 bytes to check the signature
// 34 bytes for the internal pubkey
// In Bitcoin, each byte of witness data is counted as only 1/4 of a byte towards
// the size of the transaction, so assuming the largest possible salt value, we have:
// utxo size = 41 + (145 / 4) = 77.25 bytes
// since we may add multiple utxos together, the fractional parts could add up to another byte,
// so we are rounding up to be on the safe side and set the UTXO size to 78 bytes
pub const INPUT_UTXO_SIZE_IN_BYTES: u64 = 78;

// An output contains:
// 8 bytes for the amount
// between 1 and 9 bytes for the length of the scriptPubKey
// x bytes for the scriptPubKey
// Since we only support certain types of destination addresses, we know that the
// largest supported scriptPubKey is for segwit version 1 and above, which is 42 bytes long
// so that the maximum output size is 8 + 1 + 42 = 51 bytes
pub const OUTPUT_UTXO_SIZE_IN_BYTES: u64 = 51;

// Any transaction contains:
// a 4 byte version number
// 2 bytes of flags to indicate a segwit transaction
// between 1 and 9 bytes to count the number of inputs (we assume up to 3 bytes)
// between 1 and 9 bytes to count the number of outputs (we assume up to 3 bytes)
// 4 bytes for the locktime
pub const MINIMUM_BTC_TX_SIZE_IN_BYTES: u64 = 16;

/// This determines the average expected block time that we are targeting.
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.
///
/// Change this to adjust the block time.
pub const MILLISECONDS_PER_BLOCK: u64 = 6000;

pub const SECONDS_PER_BLOCK: u64 = MILLISECONDS_PER_BLOCK / 1000;

pub const STABLE_ASSET: Asset = Asset::Usdc;

/// Determines the default (genesis) maximum allowed reduction of authority set size in
/// between two consecutive epochs.
pub const DEFAULT_MAX_AUTHORITY_SET_CONTRACTION: Percent = Percent::from_percent(30);

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
/// the initial [AccountRole::Unregistered] state.
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
	Serialize,
	Deserialize,
)]
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// !!!!!!!!!!!!!!!!!!!! IMPORTANT: Care must be taken when changing this !!!!!!!!!!!!!!!!!!!!
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!! See AccountRoles storage item !!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
pub enum AccountRole {
	/// The default account type - account not yet assigned with special role or permissions.
	#[default]
	Unregistered,
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
#[derive(
	PartialEq, Default, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
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
impl SemVer {
	/// Check if "self" is compatible with the target version.
	/// This is true if the major and minor versions are the same.
	pub fn is_compatible_with(&self, target: SemVer) -> bool {
		self.major == target.major && self.minor == target.minor
	}

	pub fn is_more_recent_than(&self, other: SemVer) -> bool {
		// This is wrapped into a function to guard against us
		// accidentally reordering the fields, for example (which
		// would be caught by tests).
		self > &other
	}
}
#[cfg(feature = "std")]
impl core::fmt::Display for SemVer {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
	}
}

/// The network environment, used to determine which chains the Chainflip network is connected to.
#[derive(
	PartialEq, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo, Default, Serialize, Deserialize,
)]
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

#[test]
fn is_more_recent_semver() {
	fn ver(major: u8, minor: u8, patch: u8) -> SemVer {
		SemVer { major, minor, patch }
	}

	fn ensure_left_is_more_recent(left: SemVer, right: SemVer) {
		assert!(left.is_more_recent_than(right));
		// Additionally check that the inverse is false:
		assert!(!right.is_more_recent_than(left));
	}

	assert!(!ver(0, 1, 0).is_more_recent_than(ver(0, 1, 0)));

	ensure_left_is_more_recent(ver(0, 0, 2), ver(0, 0, 1));
	ensure_left_is_more_recent(ver(0, 1, 0), ver(0, 0, 2));
	ensure_left_is_more_recent(ver(0, 1, 1), ver(0, 1, 0));
	ensure_left_is_more_recent(ver(0, 1, 2), ver(0, 1, 1));
	ensure_left_is_more_recent(ver(0, 2, 0), ver(0, 1, 0));
	ensure_left_is_more_recent(ver(1, 0, 0), ver(0, 2, 2));
	ensure_left_is_more_recent(ver(1, 1, 0), ver(1, 0, 2));
}
