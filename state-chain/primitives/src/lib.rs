// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![feature(int_roundings)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::sp_runtime::{
	traits::{IdentifyAccount, Verify},
	BoundedVec, MultiSignature, Percent, RuntimeDebug,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{ConstU32, Get, H256, U256};
use sp_std::{
	cmp::{Ord, PartialOrd},
	collections::btree_map::BTreeMap,
	vec::Vec,
};

pub mod basis_points;
pub mod chains;

#[macro_export]
macro_rules! define_wrapper_type {
	($name: ident, $inner: ty $(, extra_derives: $( $extra_derive: ident ),*)? ) => {

		#[derive(
			Clone,
			Copy,
			frame_support::sp_runtime::RuntimeDebug,
			PartialEq,
			Eq,
			codec::Encode,
			codec::Decode,
			codec::DecodeWithMemTracking,
			scale_info::TypeInfo,
			frame_support::pallet_prelude::MaxEncodedLen,
			Default,
			$($( $extra_derive ),*)?
		)]
		pub struct $name(pub $inner);

		impl sp_std::ops::Deref for $name {
			type Target = $inner;

			fn deref(&self) -> &Self::Target {
				&self.0
			}
		}

		impl sp_std::ops::DerefMut for $name {
			fn deref_mut(&mut self) -> &mut Self::Target {
				&mut self.0
			}
		}

		impl From<$inner> for $name {
			fn from(value: $inner) -> Self {
				$name(value)
			}
		}

		impl sp_std::fmt::Display for $name {
			fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
				write!(f, "{}", self.0)
			}
		}
	};
}

pub use chains::{assets::any::Asset, ForeignChain};

/// An index to a block.
pub type BlockNumber = u32;

/// Transaction's index within the block in which it was included.
pub type TxIndex = usize;

pub type FlipBalance = u128;

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

pub type ChannelId = u64;

pub type EgressCounter = u64;

pub type EgressId = (ForeignChain, EgressCounter);

pub type AssetAmount = u128;

pub type BasisPoints = u16;

pub type BroadcastId = u32;

/// The `log1.0001(price)` rounded to the nearest integer. Note [Price] is always
/// in units of asset One.
pub type Tick = i32;

define_wrapper_type!(SwapId, u64, extra_derives: Serialize, Deserialize, PartialOrd, Ord);

define_wrapper_type!(SwapRequestId, u64, extra_derives: Serialize, Deserialize, PartialOrd, Ord);

define_wrapper_type!(PrewitnessedDepositId, u64, extra_derives: Serialize, Deserialize, PartialOrd, Ord);

pub type BoostPoolTier = u16;

define_wrapper_type!(AffiliateShortId, u8, extra_derives: Serialize, Deserialize, PartialOrd, Ord);

/// The type of the Id given to threshold signature requests. Note a single request may
/// result in multiple ceremonies, but only one ceremony should succeed.
pub type ThresholdSignatureRequestId = u32;

pub type PolkadotBlockNumber = u32;

pub type Ed25519PublicKey = sp_core::ed25519::Public;
pub type Ipv6Addr = u128;
pub type Port = u16;

pub const FLIPPERINOS_PER_FLIP: FlipBalance = 10u128.pow(Asset::Flip.decimals());

// Bitcoin default fee, in sats per bytes, to be used if current fee is not available via chain
// tracking.
pub const DEFAULT_FEE_SATS_PER_KILOBYTE: u64 = 100000;

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

// We can spend vault UTOXs (utxos with salt=0) directly via the internal key
// as opposed to using the script path. This saves some transaction costs, because
// the witness data only consists of
// 1 byte for the number of witness elements
// 65 bytes for the signature
pub const VAULT_UTXO_SIZE_IN_BYTES: u64 = 58;

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
// 4 bytes for the lock time
pub const MINIMUM_BTC_TX_SIZE_IN_BYTES: u64 = 16;

/// This determines the average expected block time that we are targeting.
///
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.
///
/// Change this to adjust the block time.
#[cfg(any(not(feature = "turbo"), feature = "production"))]
pub const MILLISECONDS_PER_BLOCK: u64 = 6000;

/// For 'turbo' bouncer we set the block time to 1000ms.
///
/// WARNING: This MUST NOT be used in production builds,
/// as changing the block time on a live network will cause it to
/// stop. It should ONLY EVER BE USED for LOCALNET.
///
/// The feature guard makes sure that the 'turbo' and 'production'
/// features are mutually exclusive.
///
/// DO NOT TRY TO RUN 'turbo' ON A LIVE NETWORK. It will brick it.
#[cfg(all(feature = "turbo", not(feature = "production")))]
pub const MILLISECONDS_PER_BLOCK: u64 = 1000;

pub const SECONDS_PER_BLOCK: u64 = MILLISECONDS_PER_BLOCK / 1000;

/// This considers a year to have 365.25 days on average
pub const SECONDS_IN_YEAR: u32 = 60 * 60 * 24 * 365 + 60 * 60 * 24 / 4;

pub const BLOCKS_IN_YEAR: u32 = SECONDS_IN_YEAR / SECONDS_PER_BLOCK as u32;

pub const BASIS_POINTS_PER_MILLION: u32 = 100;

pub const ONE_AS_BASIS_POINTS: u16 = 10_000;

pub const STABLE_ASSET: Asset = Asset::Usdc;

/// This determines the asset ID used for USDC on Assethub
pub const ASSETHUB_USDC_ASSET_ID: u32 = 1337;

/// This determines the asset ID used for USDT on Assethub
pub const ASSETHUB_USDT_ASSET_ID: u32 = 1984;

/// Determines the default (genesis) maximum allowed reduction of authority set size in
/// between two consecutive epochs.
pub const DEFAULT_MAX_AUTHORITY_SET_CONTRACTION: Percent = Percent::from_percent(30);

// Polkadot extrinsics are uniquely identified by <block number>-<extrinsic index>
// https://wiki.polkadot.network/docs/build-protocol-info
#[derive(
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	PartialEq,
	Eq,
	Serialize,
	Deserialize,
	Ord,
	PartialOrd,
)]
pub struct TxId {
	pub block_number: PolkadotBlockNumber,
	pub extrinsic_index: u32,
}

/// The very first epoch number
pub const GENESIS_EPOCH: u32 = 1;

/// Number of blocks in the future a swap is scheduled for.
pub const SWAP_DELAY_BLOCKS: u32 = 2;

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
	DecodeWithMemTracking,
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
	/// Operators are responsible for managing delegated stake and validators signed up to their
	/// account.
	Operator,
}

pub type EgressBatch<Amount, EgressAddress> = Vec<(Amount, EgressAddress)>;

/// Struct that represents the estimated output of a Swap.
#[derive(
	PartialEq, Default, Eq, Copy, Clone, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo,
)]
pub struct SwapOutput {
	// Intermediary amount, if there's any
	pub intermediary: Option<AssetAmount>,
	// Final output of the swap
	pub output: AssetAmount,
	// the USDC network fee
	pub network_fee: AssetAmount,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
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
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct SemVer {
	pub major: u8,
	pub minor: u8,
	pub patch: u8,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum CfeCompatibility {
	/// The version is currently compatible with the target.
	Compatible,

	/// The version is not yet compatible with the target. Should wait for the new version.
	NotYetCompatible,

	/// The version of the engine is no longer compatible with the runtime. Should switch to the
	/// new version.
	NoLongerCompatible,
}

impl SemVer {
	pub fn compatibility_with_runtime(&self, version_runtime_requires: SemVer) -> CfeCompatibility {
		if self.major == version_runtime_requires.major &&
			self.minor == version_runtime_requires.minor
		{
			CfeCompatibility::Compatible
		} else if self < &version_runtime_requires {
			CfeCompatibility::NoLongerCompatible
		} else {
			CfeCompatibility::NotYetCompatible
		}
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
	PartialEq,
	Eq,
	Copy,
	Clone,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Default,
	Serialize,
	Deserialize,
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

/// Determines the Chainflip network.
#[derive(
	PartialEq, Eq, Copy, Clone, Debug, Encode, Decode, TypeInfo, Default, Serialize, Deserialize,
)]
pub enum ChainflipNetwork {
	/// Chainflip public mainnet.
	Mainnet,
	/// Chainflip public testnet
	Testnet,
	/// Chainflip development public testnet
	TestnetDev,
	/// Chainflip local development
	#[default]
	Development,
}
impl ChainflipNetwork {
	pub fn as_str(&self) -> &'static str {
		match self {
			ChainflipNetwork::Mainnet => "Chainflip-Mainnet",
			ChainflipNetwork::Testnet => "Chainflip-Testnet",
			ChainflipNetwork::TestnetDev => "Chainflip-TestnetDev",
			ChainflipNetwork::Development => "Chainflip-Development",
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

pub const MAX_AFFILIATES: u32 = 5;
// Beneficiaries can be 1 element larger since they include the primary broker:
pub const MAX_BENEFICIARIES: u32 = MAX_AFFILIATES + 1;

pub type Affiliates<Id> = BoundedVec<Beneficiary<Id>, ConstU32<MAX_AFFILIATES>>;

pub type Beneficiaries<Id> = BoundedVec<Beneficiary<Id>, ConstU32<MAX_BENEFICIARIES>>;

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	MaxEncodedLen,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
#[n_functor::derive_n_functor]
pub struct Beneficiary<Id> {
	pub account: Id,
	pub bps: BasisPoints,
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
)]
pub struct AffiliateAndFee {
	pub affiliate: AffiliateShortId,
	pub fee: u8,
}

impl From<AffiliateAndFee> for Beneficiary<AffiliateShortId> {
	fn from(AffiliateAndFee { affiliate, fee }: AffiliateAndFee) -> Self {
		Beneficiary { account: affiliate, bps: fee.into() }
	}
}

#[derive(
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
pub struct DcaParameters {
	/// The number of individual swaps to be executed
	pub number_of_chunks: u32,
	/// The interval in blocks between each swap.
	pub chunk_interval: u32,
}

pub type ShortId = u8;

pub struct StablecoinDefaults<const N: u128>();
impl<const N: u128> Get<BTreeMap<Asset, AssetAmount>> for StablecoinDefaults<N> {
	fn get() -> BTreeMap<Asset, AssetAmount> {
		Asset::all()
			.filter(|asset| asset.is_usd_stablecoin())
			.map(|asset| (asset, N))
			.collect()
	}
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum WaitFor {
	// Return immediately after the extrinsic is submitted
	NoWait,
	// Wait until the extrinsic is included in a block
	InBlock,
	// Wait until the extrinsic is in a finalized block
	#[default]
	Finalized,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ApiWaitForResult<T> {
	TxHash(H256),
	TxDetails { tx_hash: H256, response: T },
}

impl<T> ApiWaitForResult<T> {
	pub fn map_details<R>(self, f: impl FnOnce(T) -> R) -> ApiWaitForResult<R> {
		match self {
			ApiWaitForResult::TxHash(hash) => ApiWaitForResult::TxHash(hash),
			ApiWaitForResult::TxDetails { response, tx_hash } =>
				ApiWaitForResult::TxDetails { tx_hash, response: f(response) },
		}
	}

	#[track_caller]
	pub fn unwrap_details(self) -> T {
		match self {
			ApiWaitForResult::TxHash(_) => panic!("unwrap_details called on TransactionHash"),
			ApiWaitForResult::TxDetails { response, .. } => response,
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct AssetAndAmount<Amount> {
	#[serde(flatten)]
	pub asset: Asset,
	pub amount: Amount,
}

impl From<AssetAndAmount<AssetAmount>> for AssetAndAmount<U256> {
	fn from(other: AssetAndAmount<AssetAmount>) -> Self {
		Self { asset: other.asset, amount: other.amount.into() }
	}
}
/// Used in cf_ingress_egress and in cf_chains.
pub enum IngressOrEgress {
	IngressDepositChannel,
	IngressVaultSwap,
	Egress,
	EgressCcm { gas_budget: AssetAmount, message_length: usize },
}

// ------ election based witnessing ------

#[derive(
	Debug,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Deserialize,
	Serialize,
	Ord,
	PartialOrd,
)]
pub enum BlockWitnesserEvent<T> {
	PreWitness(T),
	Witness(T),
}
impl<T> BlockWitnesserEvent<T> {
	pub fn inner_witness(&self) -> &T {
		match self {
			BlockWitnesserEvent::PreWitness(w) | BlockWitnesserEvent::Witness(w) => w,
		}
	}
}
