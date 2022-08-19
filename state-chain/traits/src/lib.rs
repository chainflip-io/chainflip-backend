#![cfg_attr(not(feature = "std"), no_std)]

pub mod account_data;
mod async_result;
pub mod mocks;
pub mod offence_reporting;

use core::fmt::Debug;

pub use async_result::AsyncResult;

use cf_chains::{benchmarking_value::BenchmarkValue, ApiCall, ChainAbi, ChainCrypto};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable},
	pallet_prelude::Member,
	sp_runtime::traits::AtLeast32BitUnsigned,
	traits::{EnsureOrigin, Get, Imbalance, IsType},
	Hashable, Parameter,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Bounded, MaybeSerializeDeserialize},
	DispatchError, DispatchResult, RuntimeDebug,
};
use sp_std::{iter::Sum, marker::PhantomData, prelude::*};
/// An index to a block.
pub type BlockNumber = u32;
pub type FlipBalance = u128;
pub type EpochIndex = u32;

pub type AuthorityCount = u32;

/// Common base config for Chainflip pallets.
pub trait Chainflip:
	frame_system::Config<AccountData = account_data::ChainflipAccountData>
{
	/// An amount for a bid
	type Amount: Member
		+ Parameter
		+ MaxEncodedLen
		+ Default
		+ Eq
		+ Ord
		+ Copy
		+ AtLeast32BitUnsigned
		+ MaybeSerializeDeserialize
		+ Bounded
		+ Sum<Self::Amount>;

	/// An identity for a node
	type ValidatorId: Member
		+ Parameter
		+ MaxEncodedLen
		+ Ord
		+ core::fmt::Debug
		+ IsType<<Self as frame_system::Config>::AccountId>
		+ MaybeSerializeDeserialize;

	/// An id type for keys used in threshold signature ceremonies.
	type KeyId: Member + Parameter + From<Vec<u8>> + BenchmarkValue;
	/// The overarching call type.
	type Call: Member + Parameter + UnfilteredDispatchable<Origin = Self::Origin>;
	/// A type that allows us to check if a call was a result of witness consensus.
	type EnsureWitnessed: EnsureOrigin<Self::Origin>;
	/// A type that allows us to check if a call was a result of witness consensus by the current
	/// epoch.
	type EnsureWitnessedAtCurrentEpoch: EnsureOrigin<Self::Origin>;
	/// Information about the current Epoch.
	type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
	/// Access to information about the current system state
	type SystemState: SystemStateInfo;
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
	) -> DispatchResultWithPostInfo;
}

pub trait EpochInfo {
	/// The id type used for the validators.
	type ValidatorId;
	/// An amount
	type Amount;

	/// The last expired epoch
	fn last_expired_epoch() -> EpochIndex;

	/// The current authority set's validator ids
	fn current_authorities() -> Vec<Self::ValidatorId>;

	/// Get the current number of authorities
	fn current_authority_count() -> AuthorityCount;

	/// Gets authority index of a particular authority for a given epoch
	fn authority_index(
		epoch_index: EpochIndex,
		account: &Self::ValidatorId,
	) -> Option<AuthorityCount>;

	/// Authority count at a particular epoch.
	fn authority_count_at_epoch(epoch: EpochIndex) -> Option<AuthorityCount>;

	/// The amount to be used as bond, this is the minimum stake needed to be included in the
	/// current candidate authority set
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> EpochIndex;

	/// Are we in the auction phase of the epoch?
	fn is_auction_phase() -> bool;

	#[cfg(feature = "runtime-benchmarks")]
	fn add_authority_info_for_epoch(
		epoch_index: EpochIndex,
		new_authorities: Vec<Self::ValidatorId>,
	);
}

pub struct CurrentEpochIndex<T>(PhantomData<T>);

impl<T: Chainflip> Get<EpochIndex> for CurrentEpochIndex<T> {
	fn get() -> u32 {
		T::EpochInfo::epoch_index()
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct Bid<Id, Amount> {
	pub bidder_id: Id,
	pub amount: Amount,
}

impl<Id, Amount> From<(Id, Amount)> for Bid<Id, Amount> {
	fn from(bid: (Id, Amount)) -> Self {
		Self { bidder_id: bid.0, amount: bid.1 }
	}
}

/// The outcome of a successful auction.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct AuctionOutcome<Id, Amount> {
	/// The auction winners, sorted by in descending bid order.
	pub winners: Vec<Id>,
	/// The auction losers and their bids, sorted in descending bid order.
	pub losers: Vec<Bid<Id, Amount>>,
	/// The resulting bond for the next epoch.
	pub bond: Amount,
}

pub type RuntimeAuctionOutcome<T> =
	AuctionOutcome<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

impl<CandidateId, BidAmount: Default> Default for AuctionOutcome<CandidateId, BidAmount> {
	fn default() -> Self {
		AuctionOutcome {
			winners: Default::default(),
			losers: Default::default(),
			bond: Default::default(),
		}
	}
}

pub trait Auctioneer<T: Chainflip> {
	type Error: Into<DispatchError>;

	fn resolve_auction() -> Result<RuntimeAuctionOutcome<T>, Self::Error>;
}

pub trait VaultRotator {
	type ValidatorId;

	/// Start a vault rotation with the provided `candidates`.
	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>);

	/// Poll for the vault rotation outcome.
	fn get_vault_rotation_outcome() -> AsyncResult<Result<(), Vec<Self::ValidatorId>>>;

	#[cfg(feature = "runtime-benchmarks")]
	fn set_vault_rotation_outcome(_outcome: AsyncResult<Result<(), Vec<Self::ValidatorId>>>) {
		unimplemented!()
	}
}

/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators.
	type ValidatorId;

	/// A new epoch has started.
	fn on_new_epoch(_epoch_authorities: &[Self::ValidatorId]) {}

	/// When an epoch has been expired.
	fn on_expired_epoch(_expired: EpochIndex) {}
}

/// Resetter for Reputation Points and Online Credits of a Validator
pub trait ReputationResetter {
	type ValidatorId;

	/// Reset the reputation of a validator
	fn reset_reputation(validator: &Self::ValidatorId);
}

/// Providing bidders for an auction
pub trait BidderProvider {
	type ValidatorId;
	type Amount;
	/// Provide a list of bidders, those stakers that are not retired, with their bids which are
	/// greater than zero
	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>>;
}

pub trait StakeHandler {
	type ValidatorId;
	type Amount;

	/// A callback that is triggered after some validator's stake has changed, either by staking
	/// more Flip, or by executing a claim.
	fn on_stake_updated(validator_id: &Self::ValidatorId, new_total: Self::Amount);
}

pub trait StakeTransfer {
	type AccountId;
	type Balance;
	type Handler: StakeHandler<ValidatorId = Self::AccountId, Amount = Self::Balance>;

	/// An account's tokens that are free to be staked.
	fn staked_balance(account_id: &Self::AccountId) -> Self::Balance;

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
	/// An implementation of the issuance trait.
	type Issuance: Issuance;

	/// Distribute some rewards.
	fn distribute();
}
/// Allow triggering of emissions.
pub trait EmissionsTrigger {
	/// Trigger emissions.
	fn trigger_emissions();
}

/// Provides a unique nonce for some [Chain].
pub trait ReplayProtectionProvider<Abi: ChainAbi> {
	fn replay_protection() -> Abi::ReplayProtection;
}

/// Provides the environment data for ethereum-like chains.
pub trait EthEnvironmentProvider {
	fn flip_token_address() -> [u8; 20];
	fn key_manager_address() -> [u8; 20];
	fn stake_manager_address() -> [u8; 20];
	fn chain_id() -> u64;
}

/// A representation of the current network state for this heartbeat interval.
/// A node is regarded online if we have received a heartbeat during the last heartbeat interval
/// otherwise they are considered offline.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Default)]
pub struct NetworkState<ValidatorId> {
	/// Those nodes that are considered offline
	pub offline: Vec<ValidatorId>,
	/// Online nodes
	pub online: Vec<ValidatorId>,
}

impl<ValidatorId> NetworkState<ValidatorId> {
	/// Returns the total number of nodes in the network.
	pub fn number_of_nodes(&self) -> u32 {
		(self.online.len() + self.offline.len()) as u32
	}

	/// Return the percentage of nodes online rounded down
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
}

/// Slashing a node
pub trait Slashing {
	/// An identifier for our node
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

	/// Returns a random live signer, excluding particular provided signers. The seed value is used
	/// as a source of randomness. Returns None if no signers are live.
	fn nomination_with_seed<H: Hashable>(
		seed: H,
		exclude_ids: &[Self::SignerId],
	) -> Option<Self::SignerId>;

	/// Returns a list of live signers where the number of signers is sufficient to author a
	/// threshold signature. The seed value is used as a source of randomness.
	fn threshold_nomination_with_seed<H: Hashable>(
		seed: H,
		epoch_index: EpochIndex,
	) -> Option<Vec<Self::SignerId>>;
}

/// Provides the currently valid key for multisig ceremonies.
pub trait KeyProvider<C: ChainCrypto> {
	/// The type of the provided key_id.
	type KeyId;

	/// Gets the key id for the current key.
	fn current_key_id() -> Self::KeyId;

	/// Get the chain's current agg key.
	fn current_key() -> C::AggKey;

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(_key: C::AggKey) {
		unimplemented!()
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum RetryPolicy {
	Always,
	Never,
}

/// Api trait for pallets that need to sign things.
pub trait ThresholdSigner<C>
where
	C: ChainCrypto,
{
	type RequestId: Member + Parameter + Copy + BenchmarkValue;
	type Error: Into<DispatchError>;
	type Callback: UnfilteredDispatchable;
	type KeyId: TryInto<C::AggKey>;
	type ValidatorId;

	/// Initiate a signing request and return the request id.
	fn request_signature(payload: C::Payload) -> Self::RequestId;

	fn request_signature_with(
		key_id: Self::KeyId,
		participants: Vec<Self::ValidatorId>,
		payload: C::Payload,
		retry_policy: RetryPolicy,
	) -> Self::RequestId;

	/// Register a callback to be dispatched when the signature is available. Can fail if the
	/// provided request_id does not exist.
	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error>;

	/// Attempt to retrieve a requested signature.
	fn signature_result(
		request_id: Self::RequestId,
	) -> AsyncResult<Result<C::ThresholdSignature, ()>>;

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

	/// Helper function to enable benchmarking of the broadcast pallet
	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(_request_id: Self::RequestId, _signature: C::ThresholdSignature) {
		unimplemented!();
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
	type ValidatorId;
	type BlockNumber;
	/// Called on every heartbeat interval with the current network state
	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>);
}

/// Updating and calculating emissions per block for authorities and backup nodes
pub trait BlockEmissions {
	type Balance;
	/// Update the emissions per block for an authority
	fn update_authority_block_emission(emission: Self::Balance);
	/// Update the emissions per block for a backup node
	fn update_backup_node_block_emission(emission: Self::Balance);
	/// Calculate the emissions per block
	fn calculate_block_emissions();
}

/// Checks if the caller can execute free transactions
pub trait WaivedFees {
	type AccountId;
	type Call;
	fn should_waive_fees(call: &Self::Call, caller: &Self::AccountId) -> bool;
}

/// Qualify what is considered as a potential authority for the network
pub trait QualifyNode {
	type ValidatorId;
	/// Is the node qualified to be an authority and meet our expectations of one
	fn is_qualified(validator_id: &Self::ValidatorId) -> bool;
}

/// Qualify if the node has registered
pub struct SessionKeysRegistered<T, R>((PhantomData<T>, PhantomData<R>));

impl<T, R: frame_support::traits::ValidatorRegistration<T>> QualifyNode
	for SessionKeysRegistered<T, R>
{
	type ValidatorId = T;
	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		R::is_registered(validator_id)
	}
}

impl<A, B, C> QualifyNode for (A, B, C)
where
	A: QualifyNode<ValidatorId = B::ValidatorId>,
	B: QualifyNode,
	C: QualifyNode<ValidatorId = B::ValidatorId>,
	B::ValidatorId: Debug,
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
	fn epoch_authorities(epoch: Self::EpochIndex) -> Vec<Self::ValidatorId>;
	/// The bond for an epoch
	fn epoch_bond(epoch: Self::EpochIndex) -> Self::Amount;
	/// The unexpired epochs for which a node was in the authority set.
	fn active_epochs_for_authority(id: &Self::ValidatorId) -> Vec<Self::EpochIndex>;
	/// Removes an epoch from an authority's list of active epochs.
	fn deactivate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex);
	/// Add an epoch to an authority's list of active epochs.
	fn activate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex);
	///  Returns the amount of a authority's stake that is currently bonded.
	fn active_bond(authority: &Self::ValidatorId) -> Self::Amount;
	/// Returns the number of active epochs a authority is still active in
	fn number_of_active_epochs_for_authority(id: &Self::ValidatorId) -> u32;
}

/// Handles the bonding logic
pub trait Bonding {
	type ValidatorId;
	type Amount;
	/// Update the bond of an authority
	fn update_bond(authority: &Self::ValidatorId, bond: Self::Amount);
}
pub trait CeremonyIdProvider {
	type CeremonyId;

	/// Get the next ceremony id in the sequence.
	fn next_ceremony_id() -> Self::CeremonyId;
}

/// Something that is able to provide block authorship slots that were missed.
pub trait MissedAuthorshipSlots {
	/// Get a list of slots that were missed.
	fn missed_slots() -> sp_std::ops::Range<u64>;
}

/// Something that manages access to the system state.
pub trait SystemStateInfo {
	/// Ensure that the network is **not** in maintenance mode.
	fn ensure_no_maintenance() -> DispatchResult;

	/// Check if the network is in maintenance mode.
	fn is_maintenance_mode() -> bool {
		Self::ensure_no_maintenance().is_err()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn activate_maintenance_mode() {
		unimplemented!()
	}
}

/// Something that can manipulate the system state.
pub trait SystemStateManager {
	/// Turn system maintenance on.
	fn activate_maintenance_mode();
}

/// Allows accounts to pay for things by burning fees.
pub trait FeePayment {
	type Amount;
	type AccountId;
	/// Helper function to mint FLIP to an account.
	#[cfg(feature = "runtime-benchmarks")]
	fn mint_to_account(_account_id: &Self::AccountId, _amount: Self::Amount) {
		unreachable!()
	}
	/// Burns an amount of tokens, if the account has enough. Otherwise fails.
	fn try_burn_fee(account_id: &Self::AccountId, amount: Self::Amount) -> DispatchResult;
}

/// Provides information about the on-chain staked funds.
pub trait StakingInfo {
	type AccountId;
	type Balance;
	/// Returns the stake of an account.
	fn total_stake_of(account_id: &Self::AccountId) -> Self::Balance;
	/// Returns the total stake held on-chain.
	fn total_onchain_stake() -> Self::Balance;
}
