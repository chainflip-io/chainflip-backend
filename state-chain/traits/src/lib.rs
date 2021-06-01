#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

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

/// The phase of an Auction. At the start we are waiting on bidders, we then run an auction and
/// finally it is completed
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

/// A bid represented by a validator and the amount they wish to bid
pub type Bid<T> = (<T as Auction>::ValidatorId, <T as Auction>::Amount);
/// A range of min, max for our winning set
pub type AuctionRange = (u32, u32);

/// An Auction
///
/// An auction is broken down into three phases described by `AuctionPhase`
/// At the start we look for bidders provided by `BidderProvider` from which an auction is ran
/// This results in a set of winners and a minimum bid after the auction.  After each successful
/// call of `process()` the phase will transition else resulting in an error and preventing to move
/// on.  An confirmation is looked to before completing the auction with the `AuctionConfirmation`
/// trait.
pub trait Auction {
	type ValidatorId;
	type Amount;
	type BidderProvider;
	type Confirmation: AuctionConfirmation;

	/// Range describing auction set size
	fn auction_range() -> AuctionRange;
	/// Set the auction range
	fn set_auction_range(range: AuctionRange) -> Result<AuctionRange, AuctionError>;
	/// The current phase we find ourselves in
	fn phase() -> AuctionPhase;
	/// Move the process forward by one step, returns the phase completed or error
	fn process() -> Result<AuctionPhase, AuctionError>;
	/// The current set of bidders
	fn bidders() -> Vec<Bid<Self>>;
	/// The current/final set of winners
	fn winners() -> Vec<Self::ValidatorId>;
	/// The minimum bid needed to be included in the winners set
	fn minimum_bid() -> Self::Amount;
}

/// Confirmation of an auction
pub trait AuctionConfirmation {
	/// To confirm that the auction is valid and can continue
	fn confirmed() -> bool;
}

/// An error has occurred during an auction
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum AuctionError {
	Empty,
	MinValidatorSize,
	InvalidRange,
	NotConfirmed,
}

/// Providing bidders for our auction
pub trait BidderProvider {
	type ValidatorId;
	type Amount;
	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)>;
}

