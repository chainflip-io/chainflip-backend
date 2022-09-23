#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates

use cf_chains::eth;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

pub type IntentId = u64;

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
pub enum ForeignChain {
	Eth,
	Dot,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(eth::Address),
}

/// An Asset is a token or currency that can be traded via the Chainflip AMM.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum Asset {
	Eth,
	Flip,
	Usdc,
	Dot,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ForeignChainAsset {
	pub chain: ForeignChain,
	pub asset: Asset,
}
