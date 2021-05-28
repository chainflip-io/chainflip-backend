#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{DispatchResultWithPostInfo, Dispatchable};
use sp_std::prelude::*;
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

/// A set of validators and their stake
// pub type ValidatorSet<T> = Vec<(<T as Auction>::ValidatorId, <T as Auction>::Amount)>;

#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug)]
pub enum AuctionPhase {
	Bidders,
	Auction,
	Completed
}

impl Default for AuctionPhase {
	fn default() -> Self {
		AuctionPhase::Bidders
	}
}

pub type Bid<T> = (<T as Auction>::ValidatorId, <T as Auction>::Amount);

pub trait Auction {
	type ValidatorId;
	type Amount;

	fn phase() -> AuctionPhase;
	fn next_phase() -> Result<AuctionPhase, AuctionError>;
	fn bidders() -> Vec<Bid<Self>>;
	fn winners() -> Vec<Self::ValidatorId>;
	fn minimum_bid() -> Self::Amount;
}

#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum AuctionError {
	Empty,
	MinValidatorSize,
}

pub trait BidderProvider {
	type ValidatorId;
	type Amount;
	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)>;
}

