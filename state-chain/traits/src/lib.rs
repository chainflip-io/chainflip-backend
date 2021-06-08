#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

use codec::{Codec, Decode, Encode, FullCodec, FullEncode};
use frame_support::dispatch::{DispatchResultWithPostInfo, Dispatchable};
use frame_support::traits::ValidatorRegistration;
use sp_runtime::{DispatchError, RuntimeDebug};
use sp_std::prelude::*;

/// A trait abstracting the functionality of the witnesser
pub trait Witnesser {
	/// The type of accounts that can witness.
	type AccountId;

	/// The call type of the runtime.
	type Call: Dispatchable;

	/// Witness an event. The event is represented by a call, which should be
	/// dispatched when a threshold number of witnesses have been made.
	fn witness(who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo;
}

pub trait EpochInfo {
	/// The id type used for the validators.
	type ValidatorId;
	/// An amount
	type Amount;
	/// The index of an epoch
	type EpochIndex;

	/// The current set of validators
	fn current_validators() -> Vec<Self::ValidatorId>;

	/// Checks if the account is currently a validator.
	fn is_validator(account: &Self::ValidatorId) -> bool;

	/// If we are in auction phase then the proposed set to validate once the auction is
	/// confirmed else an empty vector
	fn next_validators() -> Vec<Self::ValidatorId>;

	/// The amount to be used as bond, this is the minimum stake needed to get into the
	/// candidate validator set
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> Self::EpochIndex;

	/// Whether or not we are currently in the auction resolution phase of the current Epoch.
	fn is_auction_phase() -> bool;
}

/// Something that can provide us a list of candidates with their corresponding stakes
pub trait CandidateProvider {
	type ValidatorId;
	type Amount;

	fn get_candidates() -> Vec<(Self::ValidatorId, Self::Amount)>;
}

/// A set of validators and their stake
pub type ValidatorSet<T> = Vec<(<T as Auction>::ValidatorId, <T as Auction>::Amount)>;
/// A proposal of validators after an auction with bond amount
pub type ValidatorProposal<T> = (Vec<<T as Auction>::ValidatorId>, <T as Auction>::Amount);

#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum AuctionError {
	BondIsZero,
	Empty,
	MinValidatorSize,
}

pub trait Auction {
	/// The id type used for the validators.
	type ValidatorId;
	/// An amount
	type Amount;
	/// A registrar to validate keys
	type Registrar: ValidatorRegistration<Self::ValidatorId>;
	/// Validate before running the auction the set of validators
	/// An empty vector is a bad bunch
	fn validate_auction(candidates: ValidatorSet<Self>) -> Result<ValidatorSet<Self>, AuctionError>;

	/// Run an auction with a set of validators returning the a proposed set of validators with the bond amount
	fn run_auction(candidates: ValidatorSet<Self>) -> Result<ValidatorProposal<Self>, AuctionError>;

	/// Complete an auction with a set of validators and accept this set and the bond for the next epoch
	fn complete_auction(proposal: ValidatorProposal<Self>) -> Result<ValidatorProposal<Self>, AuctionError>;
}

pub trait StakeTransfer {
	type AccountId;
	type Balance;

	/// An account's tokens that are free to be staked.
	fn stakeable_balance(account_id: &Self::AccountId) -> Self::Balance;

	/// Credit an account with stake from off-chain. Returns the total stake in the account.
	fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance;

	/// Reserves funds for a claim, if enough claimable funds are available.
	///
	/// Note this function makes no assumptions about how many claims may be pending simultaneously: if enough funds
	/// are available, it succeeds. Otherwise, it fails.
	fn try_claim(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError>;

	/// Same as `try_claim` but would also dip into vesting funds.
	fn try_claim_vesting(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError>;

	fn settle_claim(amount: Self::Balance);

	fn revert_claim(account_id: &Self::AccountId, amount: Self::Balance);
}

/// Trait for managing token emissions.
pub trait Emissions {
	type AccountId;
	type Balance;

	/// Burn up to `amount` of funds, or as much funds are available.
	fn burn_from(account_id: &Self::AccountId, amount: Self::Balance);

	/// Burn some funds from an account, if enough are available.
	fn try_burn_from(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError>;

	/// Mint funds to an account.
	fn mint_to(account_id: &Self::AccountId, amount: Self::Balance);

	/// Burn funds from some external (non-account) source. Use with care.
	fn vaporise(amount: Self::Balance);

	/// Returns the total issuance.
	fn total_issuance() -> Self::Balance;
}
