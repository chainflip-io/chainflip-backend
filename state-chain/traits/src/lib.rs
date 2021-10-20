#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

use cf_chains::Chain;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::Member;
use frame_support::sp_runtime::traits::AtLeast32BitUnsigned;
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable, Weight},
	traits::{EnsureOrigin, Imbalance, SignedImbalance, StoredMap},
	Parameter,
};
use frame_system::pallet_prelude::OriginFor;
use sp_runtime::{DispatchError, RuntimeDebug};
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

/// An index to a block.
pub type BlockNumber = u32;
pub type FlipBalance = u128;
/// The type used as an epoch index.
pub type EpochIndex = u32;
pub type AuctionIndex = u64;

/// Common base config for Chainflip pallets.
pub trait Chainflip: frame_system::Config {
	/// An amount for a bid
	type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
	/// An identity for a validator
	type ValidatorId: Member
		+ Parameter
		+ From<<Self as frame_system::Config>::AccountId>
		+ Into<<Self as frame_system::Config>::AccountId>;
	/// An id type for keys used in threshold signature ceremonies.
	type KeyId: Member + Parameter + From<Vec<u8>>;
	/// The overarching call type.
	type Call: Member + Parameter + UnfilteredDispatchable<Origin = Self::Origin>;
	/// A type that allows us to check if a call was a result of witness consensus.
	type EnsureWitnessed: EnsureOrigin<Self::Origin>;
}

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
	fn epoch_index() -> EpochIndex;

	/// Whether or not we are currently in the auction resolution phase of the current Epoch.
	fn is_auction_phase() -> bool;
}

/// The phase of an Auction. At the start we are waiting on bidders, we then run an auction and
/// finally it is completed
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum AuctionPhase<ValidatorId, Amount> {
	/// Waiting for bids
	WaitingForBids,
	/// Bids are now taken and validated
	BidsTaken(Vec<Bid<ValidatorId, Amount>>),
	/// We have ran the auction and have a set of validators with minimum active bid.  This waits on confirmation
	/// from the trait `VaultRotation`
	ValidatorsSelected(Vec<ValidatorId>, Amount),
	/// The confirmed set of validators
	ConfirmedValidators(Vec<ValidatorId>, Amount),
}

impl<ValidatorId, Amount: Default> Default for AuctionPhase<ValidatorId, Amount> {
	fn default() -> Self {
		AuctionPhase::WaitingForBids
	}
}

/// A bid represented by a validator and the amount they wish to bid
pub type Bid<ValidatorId, Amount> = (ValidatorId, Amount);
/// A bid that has been classified as out of the validating set
pub type RemainingBid<ValidatorId, Amount> = Bid<ValidatorId, Amount>;

/// A successful auction result
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct AuctionResult<ValidatorId, Amount> {
	pub winners: Vec<ValidatorId>,
	pub minimum_active_bid: Amount,
}

/// A range of min, max for active validator set
pub type ActiveValidatorRange = (u32, u32);

/// An Auction
///
/// An auction is broken down into three phases described by `AuctionPhase`
/// At the start we look for bidders provided by `BidderProvider` from which an auction is ran
/// This results in a set of winners and a minimum bid after the auction.  After each successful
/// call of `process()` the phase will transition else resulting in an error and preventing to move
/// on.  A confirmation is looked to before completing the auction with the `AuctionConfirmation`
/// trait.
pub trait Auctioneer {
	type ValidatorId;
	type Amount;
	type BidderProvider;

	/// Range describing auction set size
	fn active_range() -> ActiveValidatorRange;
	/// Set new auction range, returning on success the old value
	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, AuctionError>;
	/// Our last successful auction result
	fn auction_result() -> Option<AuctionResult<Self::ValidatorId, Self::Amount>>;
	/// The current phase we find ourselves in
	fn phase() -> AuctionPhase<Self::ValidatorId, Self::Amount>;
	/// Are we in an auction?
	fn waiting_on_bids() -> bool;
	/// Move our auction process to the next phase returning success with phase completed
	///
	/// At each phase we assess the bidders based on a fixed set of criteria which results
	/// in us arriving at a winning list and a bond set for this auction
	fn process() -> Result<AuctionPhase<Self::ValidatorId, Self::Amount>, AuctionError>;
	/// Abort the process and back the preliminary phase
	fn abort();
}

/// Feedback on a vault rotation
pub trait VaultRotationHandler {
	type ValidatorId;
	/// The vault rotation has been aborted
	fn vault_rotation_aborted();
	/// Penalise bad validators during a vault rotation
	fn penalise(bad_validators: &[Self::ValidatorId]);
}

/// Rotating vaults
pub trait VaultRotator {
	type ValidatorId;
	type RotationError;

	/// Start a vault rotation with the following `candidates`
	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>) -> Result<(), Self::RotationError>;

	/// In order for the validators to be rotated we are waiting on a confirmation that the vaults
	/// have been rotated.
	fn finalize_rotation() -> Result<(), Self::RotationError>;
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

/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators.
	type ValidatorId;
	type Amount: Copy;
	/// A new epoch has started
	///
	/// The `_old_validators` have moved on to leave the `_new_validators` securing the network with a
	/// `_new_bond`
	fn on_new_epoch(
		old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		new_bond: Self::Amount,
	);
}

/// Providing bidders for an auction
pub trait BidderProvider {
	type ValidatorId;
	type Amount;
	/// Provide a list of bidders
	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>>;
}

/// Trait for rotate bond after epoch.
pub trait BondRotation {
	type AccountId;
	type Balance;

	/// Sets the validator bond for all new_validator to the new_bond and
	/// the bond for all old validators to zero.
	fn update_validator_bonds(new_validators: &[Self::AccountId], new_bond: Self::Balance);
}

/// Provide feedback on staking
pub trait StakeHandler {
	type ValidatorId;
	type Amount;
	/// A validator has updated their stake and now has a new total amount
	fn stake_updated(validator_id: &Self::ValidatorId, new_total: Self::Amount);
}

pub trait StakeTransfer {
	type AccountId;
	type Balance;
	type Handler: StakeHandler<ValidatorId = Self::AccountId, Amount = Self::Balance>;

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

pub trait RewardRollover {
	type AccountId;
	/// Rolls over to another rewards period with a new set of beneficiaries, provided enough funds are available.
	fn rollover(new_beneficiaries: &[Self::AccountId]) -> Result<(), DispatchError>;
}

/// Allow triggering of emissions.
pub trait EmissionsTrigger {
	/// Trigger emissions.
	fn trigger_emissions() -> Weight;
}

/// A nonce.
pub type Nonce = u64;

/// Provides a unqiue nonce for some [Chain].
pub trait NonceProvider<C: Chain> {
	/// Get the next nonce.
	fn next_nonce() -> Nonce;
}

pub trait IsOnline {
	/// The validator id used
	type ValidatorId;
	/// The online status of the validator
	fn is_online(validator_id: &Self::ValidatorId) -> bool;
}

/// A representation of the current network state
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Default)]
pub struct NetworkState<ValidatorId> {
	/// We are missing the last heartbeat from this node and yet cannot determine if they
	/// are offline or online.
	pub missing: Vec<ValidatorId>,
	/// The node is online
	pub online: Vec<ValidatorId>,
	/// The node has been determined as being offline
	pub offline: Vec<ValidatorId>,
}

impl<ValidatorId> NetworkState<ValidatorId> {
	/// Return the percentage of validators online rounded down
	pub fn percentage_online(&self) -> u32 {
		let number_online = self.online.len() as u32;
		let number_offline = self.offline.len() as u32;

		number_online
			.saturating_mul(100)
			.checked_div(number_online + number_offline)
			.unwrap_or(0)
	}
}

/// To handle those emergency rotations
pub trait EmergencyRotation {
	/// Request an emergency rotation
	fn request_emergency_rotation() -> Weight;
	/// Is there an emergency rotation in progress
	fn emergency_rotation_in_progress() -> bool;
	/// Signal that the emergency rotation has completed
	fn emergency_rotation_completed();
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Copy)]
pub enum ChainflipAccountState {
	Passive,
	Backup,
	Validator,
}

#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug)]
pub struct ChainflipAccountData {
	pub state: ChainflipAccountState,
}

impl Default for ChainflipAccountData {
	fn default() -> Self {
		ChainflipAccountData {
			state: ChainflipAccountState::Passive,
		}
	}
}

pub trait ChainflipAccount {
	type AccountId;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData;
	fn update_state(account_id: &Self::AccountId, state: ChainflipAccountState);
}

pub struct ChainflipAccountStore<T>(PhantomData<T>);

impl<T: frame_system::Config<AccountData = ChainflipAccountData>> ChainflipAccount
	for ChainflipAccountStore<T>
{
	type AccountId = T::AccountId;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		frame_system::Pallet::<T>::get(account_id)
	}

	fn update_state(account_id: &Self::AccountId, state: ChainflipAccountState) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			(*account_data).state = state;
		})
		.expect("mutating account state")
	}
}

/// Slashing a validator
pub trait Slashing {
	/// An identifier for our validator
	type AccountId;
	/// Block number
	type BlockNumber;
	/// Function which implements the slashing logic
	fn slash(validator_id: &Self::AccountId, blocks_offline: Self::BlockNumber) -> Weight;
}

/// Something that can nominate signers from the set of active validators.
pub trait SignerNomination {
	/// The id type of signers. Most likely the same as the runtime's `ValidatorId`.
	type SignerId;

	/// Returns a random live signer. The seed value is used as a source of randomness.
	fn nomination_with_seed(seed: u64) -> Self::SignerId;

	/// Returns a list of live signers where the number of signers is sufficient to author a threshold signature. The
	/// seed value is used as a source of randomness.
	fn threshold_nomination_with_seed(seed: u64) -> Vec<Self::SignerId>;
}

/// Provides the currently valid key for multisig ceremonies.
pub trait KeyProvider<C: Chain> {
	/// The type of the provided key_id.
	type KeyId;

	/// Gets the key.
	fn current_key() -> Self::KeyId;
}

/// Api trait for pallets that need to sign things.
pub trait ThresholdSigner<T>
where
	T: Chainflip,
{
	type Context: SigningContext<T>;

	/// Initiate a signing request and return the request id.
	fn request_signature(context: Self::Context) -> u64;

	/// Initiate a transaction signing request and return the request id.
	fn request_transaction_signature<Tx: Into<Self::Context>>(transaction: Tx) -> u64 {
		Self::request_signature(transaction.into())
	}
}

/// Types, methods and state for requesting and processing a threshold signature.
pub trait SigningContext<T: Chainflip> {
	/// The chain that this context applies to.
	type Chain: Chain;
	/// The payload type that will be signed over.
	type Payload: Parameter;
	/// The signature type that is returned by the threshold signature.
	type Signature: Parameter;
	/// The callback that will be dispatched when we receive the signature.
	type Callback: UnfilteredDispatchable<Origin = T::Origin>;

	/// Returns the signing payload.
	fn get_payload(&self) -> Self::Payload;

	/// Returns the callback to be triggered on success.
	fn resolve_callback(&self, signature: Self::Signature) -> Self::Callback;

	/// Dispatches the success callback.
	fn dispatch_callback(
		&self,
		origin: OriginFor<T>,
		signature: Self::Signature,
	) -> DispatchResultWithPostInfo {
		self.resolve_callback(signature)
			.dispatch_bypass_filter(origin)
	}
}

pub mod offline_conditions {
	use super::*;
	pub type ReputationPoints = i32;

	/// Conditions that cause a validator to be knocked offline.
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
	pub enum OfflineCondition {
		/// A broadcast of an output has failed
		BroadcastOutputFailed,
		/// There was a failure in participation during a signing
		ParticipateSigningFailed,
		/// Not Enough Performance Credits
		NotEnoughPerformanceCredits,
		/// Contradicting Self During a Signing Ceremony
		ContradictingSelfDuringSigningCeremony,
	}

	/// Error on reporting an offline condition.
	#[derive(Debug, PartialEq)]
	pub enum ReportError {
		/// Validator doesn't exist
		UnknownValidator,
	}

	/// For reporting offline conditions.
	pub trait OfflineReporter {
		type ValidatorId;
		/// Report the condition for validator
		/// Returns `Ok(Weight)` else an error if the validator isn't valid
		fn report(
			condition: OfflineCondition,
			penalty: ReputationPoints,
			validator_id: &Self::ValidatorId,
		) -> Result<Weight, ReportError>;
	}
}

/// The heartbeat of the network
pub trait Heartbeat {
	type ValidatorId;
	/// A heartbeat has been submitted
	fn heartbeat_submitted(validator_id: &Self::ValidatorId) -> Weight;
	/// Called on every heartbeat interval with the current network state
	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) -> Weight;
}

/// Updating and calculating emissions per block for validators and backup validators
pub trait BlockEmissions {
	type Balance;
	/// Update the emissions per block for a validator
	fn update_validator_block_emission(emission: Self::Balance) -> Weight;
	/// Update the emissions per block for a backup validator
	fn update_backup_validator_block_emission(emission: Self::Balance) -> Weight;
	/// Calculate the emissions per block
	fn calculate_block_emissions() -> Weight;
}
