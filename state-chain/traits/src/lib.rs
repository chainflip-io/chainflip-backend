#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable, Weight},
	traits::{Imbalance, SignedImbalance},
};
use sp_runtime::{DispatchError, RuntimeDebug};
use sp_std::prelude::*;

/// A trait abstracting the functionality of the witnesser
pub trait Witnesser {
	/// The type of accounts that can witness.
	type AccountId;
	/// The call type of the runtime.
	type Call: UnfilteredDispatchable;

	/// Witness an event. The event is represented by a call, which is dispatched when a threshold number of witnesses
	/// have been made.
	///
	/// **IMPORTANT**
	/// The encoded `call` and its arguments are expected to be *unique*. If necessary this should be enforced by adding
	/// a salt or nonce to the function arguments.
	/// **IMPORTANT**
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

/// The phase of an Auction. At the start we are waiting on bidders, we then run an auction and
/// finally it is completed
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum AuctionPhase<ValidatorId, Amount> {
	// Waiting for bids, we store the last set of winners and min bid required
	WaitingForBids(Vec<ValidatorId>, Amount),
	// Bids are now taken and validated
	BidsTaken(Vec<Bid<ValidatorId, Amount>>),
	// We have ran the auction and have a set of winners with min bid.  This waits on confirmation
	// via the trait `AuctionConfirmation`
	WinnersSelected(Vec<ValidatorId>, Amount),
}

impl<ValidatorId, Amount: Default> Default for AuctionPhase<ValidatorId, Amount> {
	fn default() -> Self {
		AuctionPhase::WaitingForBids(Vec::new(), Amount::default())
	}
}

/// A bid represented by a validator and the amount they wish to bid
pub type Bid<ValidatorId, Amount> = (ValidatorId, Amount);
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

	/// Range describing auction set size
	fn auction_range() -> AuctionRange;
	/// Set the auction range
	fn set_auction_range(range: AuctionRange) -> Result<AuctionRange, AuctionError>;
	/// The current phase we find ourselves in
	fn phase() -> AuctionPhase<Self::ValidatorId, Self::Amount>;
	/// Are we in an auction?
	fn waiting_on_bids() -> bool;
	/// Move the process forward by one step, returns the phase completed or error
	fn process() -> Result<AuctionPhase<Self::ValidatorId, Self::Amount>, AuctionError>;
}

pub trait AuctionPenalty<ValidatorId> {
	/// Abort this auction
	fn abort();
	// Report on bad actors
	fn penalise(bad_validators: Vec<ValidatorId>);
}

/// Errors occurring during a rotation
#[derive(RuntimeDebug, Encode, Decode, PartialEq, Clone)]
pub enum RotationError<ValidatorId> {
	/// An invalid request index
	InvalidRequestIndex,
	/// Empty validator set provided
	EmptyValidatorSet,
	/// A set of badly acting validators
	BadValidators(Vec<ValidatorId>),
	/// The key generation response failed
	KeyResponseFailed,
	/// Failed to construct a valid chain specific payload for rotation
	FailedToConstructPayload,
	/// Vault rotation completion failed
	VaultRotationCompletionFailed,
	/// The vault rotation is not confirmed
	NotConfirmed,
	/// Failed to make keygen request
	FailedToMakeKeygenRequest,
}

/// Rotating vaults
pub trait VaultRotation {
	type ValidatorId;
	type Amount;
	// Start a vault rotation with the winners
	fn start_vault_rotation(
		winners: Vec<Self::ValidatorId>,
		min_bid: Self::Amount,
	) -> Result<(), RotationError<Self::ValidatorId>>;
	// The caller trys to finalize the rotation
	fn finalize_rotation() -> Result<(), RotationError<Self::ValidatorId>>;
}

/// An error has occurred during an auction
#[derive(Encode, Decode, Clone, Copy, RuntimeDebug, PartialEq, Eq)]
pub enum AuctionError {
	Empty,
	MinValidatorSize,
	InvalidRange,
	Abort,
	NotConfirmed,
}

/// Providing bidders for our auction
pub trait BidderProvider {
	type ValidatorId;
	type Amount;
	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)>;
}

pub trait StakeTransfer {
	type AccountId;
	type Balance;

	/// An account's tokens that are free to be staked.
	fn stakeable_balance(account_id: &Self::AccountId) -> Self::Balance;

	/// An account's tokens that are free to be claimed.
	fn claimable_balance(account_id: &Self::AccountId) -> Self::Balance;

	/// Credit an account with stake from off-chain. Returns the total stake in the account.
	fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance;

	/// Reserves funds for a claim, if enough claimable funds are available.
	///
	/// Note this function makes no assumptions about how many claims may be pending simultaneously: if enough funds
	/// are available, it succeeds. Otherwise, it fails.
	fn try_claim(account_id: &Self::AccountId, amount: Self::Balance) -> Result<(), DispatchError>;

	/// Performs any necessary settlement once a claim has been confirmed off-chain.
	fn settle_claim(amount: Self::Balance);

	/// Reverts a pending claim in the case of an expiry or cancellation.
	fn revert_claim(account_id: &Self::AccountId, amount: Self::Balance);
}

/// Trait for managing token issuance.
pub trait Issuance {
	type AccountId;
	type Balance;
	/// An imbalance representing freshly minted, unallocated funds.
	type Surplus: Imbalance<Self::Balance>;

	/// Mint new funds.
	fn mint(amount: Self::Balance) -> Self::Surplus;

	/// Burn funds from somewhere.
	fn burn(amount: Self::Balance) -> <Self::Surplus as Imbalance<Self::Balance>>::Opposite;

	/// Returns the total issuance.
	fn total_issuance() -> Self::Balance;
}

/// Distribute rewards somehow.
pub trait RewardsDistribution {
	type Balance;
	/// An imbalance representing an unallocated surplus of funds.
	type Surplus: Imbalance<Self::Balance> + Into<SignedImbalance<Self::Balance, Self::Surplus>>;

	/// Distribute some rewards.
	fn distribute(rewards: Self::Surplus);

	/// The execution weight of calling the distribution function.
	fn execution_weight() -> Weight;
}

/// Allow triggering of emissions.
pub trait EmissionsTrigger {
	/// Trigger emissions.
	fn trigger_emissions();
}

pub trait NonceProvider {
	/// A Nonce type to be used for nonces.
	type Nonce;
	/// Generates a unique nonce.
	fn generate_nonce() -> Self::Nonce;
}
