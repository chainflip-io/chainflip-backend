#![cfg_attr(not(feature = "std"), no_std)]

mod async_result;
pub mod mocks;
pub mod offence_reporting;

pub use async_result::AsyncResult;

use cf_chains::{ApiCall, ChainAbi, ChainCrypto};
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

	/// Get the current number of validators
	fn current_validator_count() -> u32;

	/// Gets validator index of a particular validator for a given epoch
	fn validator_index(epoch_index: EpochIndex, account: &Self::ValidatorId) -> Option<u16>;

	/// Validator count at a particular epoch.
	fn validator_count_at_epoch(epoch: EpochIndex) -> Option<u32>;

	/// The amount to be used as bond, this is the minimum stake needed to be included in the
	/// current candidate validator set
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> EpochIndex;

	/// Are we in the auction phase of the epoch?
	fn is_auction_phase() -> bool;

	/// The consensus threshold for the current epoch.
	///
	/// This is the number of parties required to conduct a *successful* threshold
	/// signature ceremony based on the number of active validators.
	fn consensus_threshold() -> u32 {
		cf_utilities::success_threshold_from_share_count(Self::current_validator_count() as u32)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_validator_index(epoch_index: EpochIndex, account: &Self::ValidatorId, index: u16);

	#[cfg(feature = "runtime-benchmarks")]
	fn set_validator_count_for_epoch(epoch: EpochIndex, count: u32);
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

/// A bid represented by a validator and the amount they wish to bid
pub type Bid<ValidatorId, Amount> = (ValidatorId, Amount);
/// A bid that has been classified as out of the validating set
pub type RemainingBid<ValidatorId, Amount> = Bid<ValidatorId, Amount>;

/// The outcome of a successful auction.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
pub struct AuctionOutcome<T: Chainflip> {
	/// The auction winners.
	pub winners: Vec<T::ValidatorId>,
	/// The auction losers and their bids.
	pub losers: Vec<(T::ValidatorId, T::Amount)>,
	/// The resulting bond for the next epoch.
	pub bond: T::Amount,
}

pub trait Auctioneer<T: Chainflip> {
	type Error: Into<DispatchError>;

	fn resolve_auction() -> Result<AuctionOutcome<T>, Self::Error>;
}

pub trait BackupValidators {
	type ValidatorId;

	/// The current set of backup validators.  The set may change at anytime.
	fn backup_validators() -> Vec<Self::ValidatorId>;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum SuccessOrFailure {
	Success,
	Failure,
}

/// Rotating vaults
pub trait VaultRotator {
	type ValidatorId;
	type RotationError: Into<DispatchError>;

	/// Start a vault rotation with the following `candidates`
	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>) -> Result<(), Self::RotationError>;

	/// Get the status of the current key generation
	fn get_vault_rotation_outcome() -> AsyncResult<SuccessOrFailure>;
}

/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators.
	type ValidatorId;
	/// A new epoch has started
	///
	/// The `previous_epoch_validators` now let `epoch_validators` take control
	/// There can be an overlap between these two sets of validators
	fn on_new_epoch(epoch_validators: &[Self::ValidatorId]);
}

/// Providing bidders for an auction
pub trait BidderProvider {
	type ValidatorId;
	type Amount;
	/// Provide a list of bidders, those stakers that are not retired, with their bids which are
	/// greater than zero
	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>>;
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

	/// The amount of locked tokens in the current epoch - aka the bond
	fn locked_balance(account_id: &Self::AccountId) -> Self::Balance;

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

/// Provides a unqiue nonce for some [Chain].
pub trait NonceProvider<Abi: ChainAbi> {
	/// Get the next nonce.
	fn next_nonce() -> Abi::Nonce;
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
pub enum BackupOrPassive {
	Backup,
	Passive,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Copy)]
pub enum ChainflipAccountState {
	CurrentAuthority,
	HistoricalAuthority(BackupOrPassive),
	BackupOrPassive(BackupOrPassive),
}

// TODO: Just use the AccountState
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug)]
pub struct ChainflipAccountData {
	pub state: ChainflipAccountState,
}

impl Default for ChainflipAccountData {
	fn default() -> Self {
		ChainflipAccountData {
			state: ChainflipAccountState::BackupOrPassive(BackupOrPassive::Passive),
		}
	}
}

pub trait ChainflipAccount {
	type AccountId;

	/// Get the account data for the given account id.
	fn get(account_id: &Self::AccountId) -> ChainflipAccountData;
	/// Updates the state of a
	fn set_backup_or_passive(account_id: &Self::AccountId, backup_or_passive: BackupOrPassive);
	/// Set the validator to be the current authority
	fn set_current_authority(account_id: &Self::AccountId);
	/// Sets the validator state to historical
	fn set_historical_validator(account_id: &Self::AccountId);
	/// Sets the current validator to the historical validator, should be called
	/// once the validator has no more active epochs
	fn from_historical_to_backup_or_passive(account_id: &Self::AccountId);
}

// Remove in place of a proper validator enum
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

	fn set_backup_or_passive(account_id: &Self::AccountId, state: BackupOrPassive) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| match account_data.state {
			ChainflipAccountState::CurrentAuthority => {
				log::warn!("Attempted to set backup or passive on a current authority account");
			},
			ChainflipAccountState::HistoricalAuthority(_) => {
				(*account_data).state = ChainflipAccountState::HistoricalAuthority(state);
			},
			ChainflipAccountState::BackupOrPassive(_) => {
				(*account_data).state = ChainflipAccountState::BackupOrPassive(state);
			},
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}

	/// Set the last epoch number and set the account state to Validator
	fn set_current_authority(account_id: &Self::AccountId) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			(*account_data).state = ChainflipAccountState::CurrentAuthority;
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}

	// TODO: How to check if we set to backup or passive
	// we might want to combine this with an update_backup_or_passive
	fn set_historical_validator(account_id: &Self::AccountId) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			(*account_data).state =
				ChainflipAccountState::HistoricalAuthority(BackupOrPassive::Passive);
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}

	fn from_historical_to_backup_or_passive(account_id: &Self::AccountId) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| match account_data.state {
			ChainflipAccountState::HistoricalAuthority(state) => {
				(*account_data).state = ChainflipAccountState::BackupOrPassive(state);
			},
			_ => {
				log::error!(
					"Attempted to set backup or passive on a CurrentAuthority or BackupOrPassive"
				);
			},
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
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

/// Something that is capable of encoding and broadcasting native blockchain api calls to external
/// chains.
pub trait Broadcaster<Api: ChainAbi> {
	/// Supported api calls for this chain.
	type ApiCall: ApiCall<Api>;

	/// Request a threshold signature and then build and broadcast the outbound api call.
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall);
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

/// Provides an interface to all passed epochs
pub trait HistoricalEpoch {
	type ValidatorId;
	type EpochIndex;
	type Amount;
	/// All validators which were in an epoch's authority set.
	fn epoch_validators(epoch: Self::EpochIndex) -> Vec<Self::ValidatorId>;
	/// The bond for an epoch
	fn epoch_bond(epoch: Self::EpochIndex) -> Self::Amount;
	/// The unexpired epochs for which a validator was in the authority set.
	fn active_epochs_for_validator(id: &Self::ValidatorId) -> Vec<Self::EpochIndex>;
	/// Removes an epoch from a validator's list of active epochs.
	fn deactivate_epoch(validator: &Self::ValidatorId, epoch: EpochIndex);
	/// Add an epoch to a validator's list of active epochs.
	fn activate_epoch(validator: &Self::ValidatorId, epoch: EpochIndex);
	///  Returns the amount of a validator's stake that is currently bonded.
	fn active_bond(validator: &Self::ValidatorId) -> Self::Amount;
	/// Returns the number of active epochs a validator is still active in
	fn number_of_active_epochs_for_validator(id: &Self::ValidatorId) -> u32;
}

/// Handles the expiry of an epoch
pub trait EpochExpiry {
	fn expire_epoch(epoch: EpochIndex);
}

/// Handles the bonding logic
pub trait Bonding {
	type ValidatorId;
	type Amount;
	/// Update the bond of an validator
	fn update_validator_bond(validator: &Self::ValidatorId, bond: Self::Amount);
}
pub trait CeremonyIdProvider {
	type CeremonyId;

	/// Get the next ceremony id in the sequence.
	fn next_ceremony_id() -> Self::CeremonyId;
}

/// Something that is able to provide block authorship slots that were missed.
pub trait MissedAuthorshipSlots {
	/// Get a list of slots that were missed.
	fn missed_slots() -> Vec<u64>;
}
