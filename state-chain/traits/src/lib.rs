#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

use cf_chains::{ApiCall, Chain, ChainApi, ChainCrypto};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable, Weight},
	pallet_prelude::Member,
	sp_runtime::traits::AtLeast32BitUnsigned,
	traits::{EnsureOrigin, Get, Imbalance, SignedImbalance, StoredMap},
	Hashable, Parameter,
};
use sp_runtime::{DispatchError, RuntimeDebug};
use sp_std::{marker::PhantomData, prelude::*};

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
		+ Default
		+ Parameter
		+ Ord
		+ core::fmt::Debug
		+ From<<Self as frame_system::Config>::AccountId>
		+ Into<<Self as frame_system::Config>::AccountId>;

	/// An id type for keys used in threshold signature ceremonies.
	type KeyId: Member + Parameter + From<Vec<u8>>;
	/// The overarching call type.
	type Call: Member + Parameter + UnfilteredDispatchable<Origin = Self::Origin>;
	/// A type that allows us to check if a call was a result of witness consensus.
	type EnsureWitnessed: EnsureOrigin<Self::Origin>;
	/// Information about the current Epoch.
	type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
}

/// A trait abstracting the functionality of the witnesser
pub trait Witnesser {
	/// The type of accounts that can witness.
	type AccountId;
	/// The call type of the runtime.
	type Call: UnfilteredDispatchable;

	/// Witness an event. The event is represented by a call, which is dispatched when a threshold
	/// number of witnesses have been made.
	///
	/// **IMPORTANT**
	/// The encoded `call` and its arguments are expected to be *unique*. If necessary this should
	/// be enforced by adding a salt or nonce to the function arguments.
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

	/// The amount to be used as bond, this is the minimum stake needed to be included in the
	/// current candidate validator set
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> EpochIndex;

	/// Whether or not we are currently in the auction resolution phase of the current Epoch.
	fn is_auction_phase() -> bool;

	/// The number of validators in the current active set.
	fn active_validator_count() -> u32;

	/// The consensus threshold for the current epoch.
	///
	/// This is the number of parties required to conduct a *successful* threshold
	/// signature ceremony based on the number of active validators.
	fn consensus_threshold() -> u32 {
		cf_utilities::success_threshold_from_share_count(Self::active_validator_count())
	}
}

pub struct CurrentThreshold<T>(PhantomData<T>);

impl<T: Chainflip> Get<u32> for CurrentThreshold<T> {
	fn get() -> u32 {
		T::EpochInfo::consensus_threshold()
	}
}

pub struct CurrentEpochIndex<T>(PhantomData<T>);

impl<T: Chainflip> Get<EpochIndex> for CurrentEpochIndex<T> {
	fn get() -> u32 {
		T::EpochInfo::epoch_index()
	}
}

/// The phase of an Auction. At the start we are waiting on bidders, we then run an auction and
/// finally it is completed
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum AuctionPhase<ValidatorId, Amount> {
	/// Waiting for bids
	WaitingForBids,
	/// We have ran the auction and have a set of validators with minimum active bid awaiting
	/// confirmation
	ValidatorsSelected(Vec<ValidatorId>, Amount),
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

	/// The last auction ran
	fn auction_index() -> AuctionIndex;
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

pub trait BackupValidators {
	type ValidatorId;

	/// The current set of backup validators.  The set may change at anytime.
	fn backup_validators() -> Vec<Self::ValidatorId>;
}

/// Feedback on a vault rotation
pub trait VaultRotationHandler {
	type ValidatorId;
	/// The vault rotation has been aborted
	fn vault_rotation_aborted();
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
	/// The current epoch is ending
	fn on_epoch_ending() {}
	/// A new epoch has started
	///
	/// The `old_validators` have moved on to leave the `new_validators` securing the network with
	/// a `new_bond`
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
	/// Provide a list of bidders, those stakers that are not retired
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
	/// Note this function makes no assumptions about how many claims may be pending simultaneously:
	/// if enough funds are available, it succeeds. Otherwise, it fails.
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
	/// Rolls over to another rewards period with a new set of beneficiaries, provided enough funds
	/// are available.
	fn rollover(new_beneficiaries: &[Self::AccountId]) -> Result<(), DispatchError>;
}

pub trait Rewarder {
	type AccountId;
	// Apportion rewards due to all beneficiaries
	fn reward_all() -> Result<(), DispatchError>;
}

/// Allow triggering of emissions.
pub trait EmissionsTrigger {
	/// Trigger emissions.
	fn trigger_emissions() -> Weight;
}

/// A nonce.
pub type Nonce = u64;

/// Provides a unqiue nonce for some [Chain].
/// TODO: Implement for a generic nonce type via ChainApi.
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

pub trait HasPeerMapping {
	/// The validator id used
	type ValidatorId;
	/// The existence of this validators peer mapping
	fn has_peer_mapping(validator_id: &Self::ValidatorId) -> bool;
}

/// A representation of the current network state for this heartbeat interval.
/// A node is regarded online if we have received a heartbeat during the last heartbeat interval
/// otherwise they are considered offline.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Default)]
pub struct NetworkState<ValidatorId: Default> {
	/// Those nodes that are considered offline
	pub offline: Vec<ValidatorId>,
	/// Online nodes
	pub online: Vec<ValidatorId>,
}

impl<ValidatorId: Default> NetworkState<ValidatorId> {
	/// Return the number of nodes with state Validator in the network
	pub fn number_of_nodes(&self) -> u32 {
		(self.online.len() + self.offline.len()) as u32
	}

	/// Return the percentage of validators online rounded down
	pub fn percentage_online(&self) -> u32 {
		let number_online = self.online.len() as u32;

		number_online
			.saturating_mul(100)
			.checked_div(self.number_of_nodes())
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
	pub last_active_epoch: Option<EpochIndex>,
}

impl Default for ChainflipAccountData {
	fn default() -> Self {
		ChainflipAccountData { state: ChainflipAccountState::Passive, last_active_epoch: None }
	}
}

pub trait ChainflipAccount {
	type AccountId;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData;
	fn update_state(account_id: &Self::AccountId, state: ChainflipAccountState);
	fn update_last_active_epoch(account_id: &Self::AccountId, index: EpochIndex);
}

/// An outgoing node
pub trait IsOutgoing {
	type AccountId;

	/// Returns true if this account is an outgoer which by definition is a node that was in the
	/// active set in the *last* epoch
	fn is_outgoing(account_id: &Self::AccountId) -> bool;
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

	fn update_last_active_epoch(account_id: &Self::AccountId, index: EpochIndex) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			(*account_data).last_active_epoch = Some(index);
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
	/// Returns None if no signers are live.
	fn nomination_with_seed<H: Hashable>(seed: H) -> Option<Self::SignerId>;

	/// Returns a list of live signers where the number of signers is sufficient to author a
	/// threshold signature. The seed value is used as a source of randomness.
	fn threshold_nomination_with_seed<H: Hashable>(seed: H) -> Option<Vec<Self::SignerId>>;
}

/// Provides the currently valid key for multisig ceremonies.
pub trait KeyProvider<C: ChainCrypto> {
	/// The type of the provided key_id.
	type KeyId;

	/// Gets the key id for the current key.
	fn current_key_id() -> Self::KeyId;

	/// Get the chain's current agg key.
	fn current_key() -> C::AggKey;
}

/// Api trait for pallets that need to sign things.
pub trait ThresholdSigner<C>
where
	C: ChainCrypto,
{
	type RequestId: Member + Parameter + Copy;
	type Error: Into<DispatchError>;
	type Callback: UnfilteredDispatchable;

	/// Initiate a signing request and return the request id.
	fn request_signature(payload: C::Payload) -> Self::RequestId;

	/// Register a callback to be dispatched when the signature is available. Can fail if the
	/// provided request_id does not exist.
	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error>;

	/// Attempt to retrieve a requested signature.
	fn signature_result(request_id: Self::RequestId) -> AsyncResult<C::ThresholdSignature>;

	/// Request a signature and register a callback for when the signature is available.
	///
	/// Since the callback is registered immediately, it should never fail.
	///
	/// Note that the `callback_generator` closure is *not* the callback. It is what *generates*
	/// the callback based on the request id.
	fn request_signature_with_callback(
		payload: C::Payload,
		callback_generator: impl FnOnce(Self::RequestId) -> Self::Callback,
	) -> Self::RequestId {
		let id = Self::request_signature(payload);
		Self::register_callback(id, callback_generator(id)).unwrap_or_else(|e| {
			log::error!(
				"Unable to register threshold signature callback. This should not be possible. Error: '{:?}'",
				e.into()
			);
		});
		id
	}
}

#[derive(Clone, Copy, RuntimeDebug, Encode, Decode, PartialEq, Eq)]
pub enum AsyncResult<R> {
	/// Result is ready.
	Ready(R),
	/// Result is requested but not available. (still being generated)
	Pending,
	/// Result is void. (not yet requested or has already been used)
	Void,
}

impl<R> AsyncResult<R> {
	pub fn ready_or_else<E>(self, e: impl FnOnce(Self) -> E) -> Result<R, E> {
		match self {
			AsyncResult::Ready(s) => Ok(s),
			_ => Err(e(self)),
		}
	}
}

impl<R> Default for AsyncResult<R> {
	fn default() -> Self {
		Self::Void
	}
}

/// Something that is capable of encoding and broadcasting native blockchain api calls to external
/// chains.
pub trait Broadcaster<Api: ChainApi> {
	/// Supported api calls for this chain.
	type ApiCall: ApiCall<Api>;

	/// Request a threshold signature and then build and broadcast the outbound api call.
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall);
}

pub mod offline_conditions {
	use super::*;
	pub type ReputationPoints = i32;

	/// Conditions that cause a validator to be docked reputation points
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
	pub enum OfflineCondition {
		/// There was a failure in participation during a signing
		ParticipateSigningFailed,
		/// There was a failure in participation during a key generation ceremony
		ParticipateKeygenFailed,
		/// An invalid transaction was authored
		InvalidTransactionAuthored,
		/// A transaction failed on transmission
		TransactionFailedOnTransmission,
	}

	/// Error on reporting an offline condition.
	#[derive(Debug, PartialEq)]
	pub enum ReportError {
		/// Validator doesn't exist
		UnknownValidator,
	}

	pub trait OfflinePenalty {
		fn penalty(condition: &OfflineCondition) -> (ReputationPoints, bool);
	}

	/// For reporting offline conditions.
	pub trait OfflineReporter {
		type ValidatorId;
		type Penalty: OfflinePenalty;

		/// Report the condition for validator
		/// Returns `Ok(Weight)` else an error if the validator isn't valid
		fn report(
			condition: OfflineCondition,
			validator_id: &Self::ValidatorId,
		) -> Result<Weight, ReportError>;
	}

	/// We report on nodes that should be banned
	pub trait Banned {
		type ValidatorId;
		/// A validator to be banned
		fn ban(validator_id: &Self::ValidatorId);
	}
}

/// The heartbeat of the network
pub trait Heartbeat {
	type ValidatorId: Default;
	type BlockNumber;
	/// A heartbeat has been submitted
	fn heartbeat_submitted(
		validator_id: &Self::ValidatorId,
		block_number: Self::BlockNumber,
	) -> Weight;
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

/// Checks if the caller can execute free transactions
pub trait WaivedFees {
	type AccountId;
	type Call;
	fn should_waive_fees(call: &Self::Call, caller: &Self::AccountId) -> bool;
}

/// Qualify what is considered as a potential validator for the network
pub trait QualifyValidator {
	type ValidatorId;
	/// Is the validator qualified to be a validator and meet our expectations of one
	fn is_qualified(validator_id: &Self::ValidatorId) -> bool;
}
