#![cfg_attr(not(feature = "std"), no_std)]

pub mod mocks;

use cf_chains::{Chain, ChainCrypto};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable},
	pallet_prelude::Member,
	sp_runtime::traits::AtLeast32BitUnsigned,
	traits::{EnsureOrigin, Get, Imbalance, StoredMap},
	Hashable, Parameter,
};
use sp_runtime::{traits::MaybeSerializeDeserialize, DispatchError, RuntimeDebug};
use sp_std::{marker::PhantomData, prelude::*};
/// An index to a block.
pub type BlockNumber = u32;
pub type FlipBalance = u128;
/// The type used as an epoch index.
pub type EpochIndex = u32;

/// Common base config for Chainflip pallets.
pub trait Chainflip: frame_system::Config {
	/// An amount for a bid
	type Amount: Member
		+ Parameter
		+ Default
		+ Eq
		+ Ord
		+ Copy
		+ AtLeast32BitUnsigned
		+ MaybeSerializeDeserialize;

	/// An identity for a validator
	type ValidatorId: Member
		+ Default
		+ Parameter
		+ Ord
		+ core::fmt::Debug
		+ From<<Self as frame_system::Config>::AccountId>
		+ Into<<Self as frame_system::Config>::AccountId>
		+ MaybeSerializeDeserialize;

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
	/// The type for block numbers
	type BlockNumber;

	/// Witness an event. The event is represented by a call, which is dispatched when a threshold
	/// number of witnesses have been made.
	///
	/// **IMPORTANT**
	/// The encoded `call` and its arguments are expected to be *unique*. If necessary this should
	/// be enforced by adding a salt or nonce to the function arguments.
	/// **IMPORTANT**
	fn witness(who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo;
	/// Witness an event, as above, during a specific epoch
	fn witness_at_epoch(
		who: Self::AccountId,
		call: Self::Call,
		epoch: EpochIndex,
		block_number: Self::BlockNumber,
	) -> DispatchResultWithPostInfo;
}

pub trait EpochInfo {
	/// The id type used for the validators.
	type ValidatorId;
	/// An amount
	type Amount;

	/// The last expired epoch
	fn last_expired_epoch() -> EpochIndex;

	/// The current set of validators
	fn current_validators() -> Vec<Self::ValidatorId>;

	/// Checks if the account is currently a validator.
	fn is_validator(account: &Self::ValidatorId) -> bool;

	/// The amount to be used as bond, this is the minimum stake needed to be included in the
	/// current candidate validator set
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> EpochIndex;

	/// Are we in the auction phase of the epoch?
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
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
pub struct AuctionResult<ValidatorId, Amount> {
	pub winners: Vec<ValidatorId>,
	pub minimum_active_bid: Amount,
}

/// A range of min, max for active validator set
pub type ActiveValidatorRange = (u32, u32);

/// Auctioneer
///
/// The auctioneer is responsible in running and confirming an auction.  Bidders are selected and
/// returned as an `AuctionResult` calling `run_aucion()`. The result would then be provided to
/// `update_validator_status()` when have rotated the active set.  A new auction is ran on each call
/// of `resolve_auction()` which discards the previous auction run.

pub trait Auctioneer {
	type ValidatorId;
	type Amount;

	/// Run an auction by qualifying a validator
	fn resolve_auction() -> Result<AuctionResult<Self::ValidatorId, Self::Amount>, AuctionError>;
	/// Update validator status for the winners
	fn update_validator_status(winners: &[Self::ValidatorId]);
}

pub trait BackupValidators {
	type ValidatorId;

	/// The current set of backup validators.  The set may change at anytime.
	fn backup_validators() -> Vec<Self::ValidatorId>;
}

#[derive(PartialEq, Eq, Clone, Encode, Decode)]
pub enum KeygenStatus {
	Busy,
	Failed,
}

/// Rotating vaults
pub trait VaultRotator {
	type ValidatorId;
	type RotationError: Into<DispatchError>;

	/// Start a vault rotation with the following `candidates`
	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>) -> Result<(), Self::RotationError>;

	/// Get the status of the current key generation
	fn get_keygen_status() -> Option<KeygenStatus>;
}

/// An error has occurred during an auction
#[derive(Encode, Decode, Clone, Copy, RuntimeDebug, PartialEq, Eq)]
pub enum AuctionError {
	/// Insufficient number of bidders
	NotEnoughBidders,
}

impl Into<DispatchError> for AuctionError {
	fn into(self) -> DispatchError {
		match self {
			AuctionError::NotEnoughBidders => DispatchError::Other("NotEnoughBidders"),
		}
	}
}
/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators.
	type ValidatorId;
	type Amount: Copy;
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
	/// Provide a list of bidders, those stakers that are not retired, with their bids which are
	/// greater than zero
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
	type Surplus: Imbalance<Self::Balance>;

	/// Distribute some rewards.
	fn distribute(rewards: Self::Surplus);
}
/// Allow triggering of emissions.
pub trait EmissionsTrigger {
	/// Trigger emissions.
	fn trigger_emissions();
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
	fn request_emergency_rotation();
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
	fn slash(validator_id: &Self::AccountId, blocks_offline: Self::BlockNumber);
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
	type Chain: Chain + ChainCrypto;
	/// The callback that will be dispatched when we receive the signature.
	type Callback: UnfilteredDispatchable<Origin = T::Origin>;
	/// The origin that is authorised to dispatch the callback, ie. the origin that represents
	/// a valid, verifiied, threshold signature.
	type ThresholdSignatureOrigin: Into<T::Origin>;

	/// Returns the signing payload.
	fn get_payload(&self) -> <Self::Chain as ChainCrypto>::Payload;

	/// Returns the callback to be triggered on success.
	fn resolve_callback(
		&self,
		signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> Self::Callback;

	/// Dispatches the success callback.
	fn dispatch_callback(
		&self,
		origin: Self::ThresholdSignatureOrigin,
		signature: <Self::Chain as ChainCrypto>::ThresholdSignature,
	) -> DispatchResultWithPostInfo {
		self.resolve_callback(signature).dispatch_bypass_filter(origin.into())
	}
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

	pub trait OfflinePenalty {
		fn penalty(condition: &OfflineCondition) -> (ReputationPoints, bool);
	}

	/// For reporting offline conditions.
	pub trait OfflineReporter {
		type ValidatorId;
		type Penalty: OfflinePenalty;

		/// Report the condition for validator
		/// Returns `Ok(Weight)` else an error if the validator isn't valid
		fn report(condition: OfflineCondition, validator_id: &Self::ValidatorId);
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
	fn heartbeat_submitted(validator_id: &Self::ValidatorId, block_number: Self::BlockNumber);
	/// Called on every heartbeat interval with the current network state
	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>);
}

/// Updating and calculating emissions per block for validators and backup validators
pub trait BlockEmissions {
	type Balance;
	/// Update the emissions per block for a validator
	fn update_validator_block_emission(emission: Self::Balance);
	/// Update the emissions per block for a backup validator
	fn update_backup_validator_block_emission(emission: Self::Balance);
	/// Calculate the emissions per block
	fn calculate_block_emissions();
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

/// A *not* qualified validator
pub struct NotQualifiedValidator<T>(PhantomData<T>);

impl<T> QualifyValidator for NotQualifiedValidator<T> {
	type ValidatorId = T;
	fn is_qualified(_: &Self::ValidatorId) -> bool {
		true
	}
}

/// Qualify if the validator has registered
pub struct SessionKeysRegistered<T, R>((PhantomData<T>, PhantomData<R>));

impl<T, R: frame_support::traits::ValidatorRegistration<T>> QualifyValidator
	for SessionKeysRegistered<T, R>
{
	type ValidatorId = T;
	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		R::is_registered(validator_id)
	}
}

impl<A, B, C> QualifyValidator for (A, B, C)
where
	A: QualifyValidator<ValidatorId = B::ValidatorId>,
	B: QualifyValidator,
	C: QualifyValidator<ValidatorId = B::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		A::is_qualified(validator_id) &&
			B::is_qualified(validator_id) &&
			C::is_qualified(validator_id)
	}
}
/// Handles the check of execution conditions
pub trait ExecutionCondition {
	/// Returns true/false if the condition is satisfied
	fn is_satisfied() -> bool;
}

/// Performs a runtime upgrade
pub trait RuntimeUpgrade {
	/// Applies the wasm code of a runtime upgrade and returns the
	/// information about the execution
	fn do_upgrade(code: Vec<u8>) -> DispatchResultWithPostInfo;
}
