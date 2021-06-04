#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

use frame_support::dispatch::{DispatchResultWithPostInfo, Dispatchable};
use sp_std::prelude::*;
use frame_support::traits::ValidatorRegistration;
use codec::{Encode, Decode};
use sp_runtime::RuntimeDebug;
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

pub trait Action {}

pub trait Reporter {
	type AccountId;
	type Action: Action;

	fn add_account(account_id: &Self::AccountId) -> Result<(), JudgementError>;
	fn remove_account(account_id: &Self::AccountId) -> Result<(), JudgementError>;
	fn report(account_id: &Self::AccountId, action: Self::Action) -> Result<(), JudgementError>;
}

#[derive(Debug, Eq, PartialEq)]
pub enum JudgementError {
	AccountNotFound,
	AccountExists,
}

pub trait Judgement<T: Reporter, BlockNumber> {
	fn liveliness(account_id: &T::AccountId) -> Result<BlockNumber, JudgementError>;
	fn report_for(account_id: &T::AccountId) -> Result<Vec<T::Action>, JudgementError>;
	fn clean_all(account_id: &T::AccountId) -> Result<(), JudgementError>;
}