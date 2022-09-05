#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates

use cf_chains::eth;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{AccountId32, Permill, RuntimeDebug};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ChainflipAccountState {
	CurrentAuthority,
	/// Historical implies backup too
	HistoricalAuthority,
	Backup,
}

impl ChainflipAccountState {
	pub fn is_authority(&self) -> bool {
		matches!(self, ChainflipAccountState::CurrentAuthority)
	}

	pub fn is_backup(&self) -> bool {
		matches!(self, ChainflipAccountState::HistoricalAuthority | ChainflipAccountState::Backup)
	}
}

// TODO: Just use the AccountState
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ChainflipAccountData {
	pub state: ChainflipAccountState,
}

impl Default for ChainflipAccountData {
	fn default() -> Self {
		ChainflipAccountData { state: ChainflipAccountState::Backup }
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum ForeignChain {
	Eth,
	Dot,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum ForeignChainAddress {
	Eth(eth::Address),
}

/// These assets can be on multiple chains.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum ForeignAsset {
	Eth,
	Flip,
	Usdc,
	Dot,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct ForeignChainAsset {
	chain: ForeignChain,
	asset: ForeignAsset,
}

/// The intent id just needs to be unique for each intent.
pub type IntentId = u64;

pub struct IntentCommon {
	_intent_id: IntentId,
	_ingress_asset: ChainAsset,
}

/// There are two types of ingress intent.
pub enum Intent {
	Swap {
		intent_common: IntentCommon,
		egress_asset: ChainAsset,
		egress_address: ChainAddress,
		relayer_fee: Permill,
	},
	LiquidityProvision {
		intent_common: IntentCommon,
		lp_account: AccountId32,
	},
}
