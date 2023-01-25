#![cfg_attr(not(feature = "std"), no_std)]

mod async_result;
pub mod liquidity;
pub use liquidity::*;

pub mod mocks;
pub mod offence_reporting;

use core::fmt::Debug;

pub use async_result::AsyncResult;
use sp_std::collections::btree_set::BTreeSet;

use cf_chains::{
	benchmarking_value::BenchmarkValue, ApiCall, Chain, ChainAbi, ChainCrypto, Ethereum, Polkadot,
};

use cf_primitives::{
	chains::assets, AccountRole, Asset, AssetAmount, AuthorityCount, BroadcastId, CeremonyId,
	EgressId, EpochIndex, EthereumAddress, ForeignChain, ForeignChainAddress, IntentId,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable},
	error::BadOrigin,
	pallet_prelude::Member,
	traits::{EnsureOrigin, Get, Imbalance, IsType},
	Hashable, Parameter,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, Bounded, MaybeSerializeDeserialize},
	DispatchError, DispatchResult, FixedPointOperand, RuntimeDebug,
};
use sp_std::{iter::Sum, marker::PhantomData, prelude::*};

/// Common base config for Chainflip pallets.
pub trait Chainflip: frame_system::Config {
	/// An amount for a bid
	type Amount: Member
		+ Parameter
		+ MaxEncodedLen
		+ Default
		+ Eq
		+ Ord
		+ Copy
		+ AtLeast32BitUnsigned
		+ FixedPointOperand
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
	type RuntimeCall: Member
		+ Parameter
		+ UnfilteredDispatchable<RuntimeOrigin = Self::RuntimeOrigin>;
	/// A type that allows us to check if a call was a result of witness consensus.
	type EnsureWitnessed: EnsureOrigin<Self::RuntimeOrigin>;
	/// A type that allows us to check if a call was a result of witness consensus by the current
	/// epoch.
	type EnsureWitnessedAtCurrentEpoch: EnsureOrigin<Self::RuntimeOrigin>;
	/// Information about the current Epoch.
	type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
	/// Access to information about the current system state
	type SystemState: SystemStateInfo;
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

#[derive(PartialEq, Eq, Clone, Debug, Decode, Encode)]
pub enum VaultStatus<ValidatorId> {
	KeygenComplete,
	RotationComplete,
	Failed(BTreeSet<ValidatorId>),
}

pub trait VaultRotator {
	type ValidatorId: Ord + Clone;

	/// Start the rotation by kicking off keygen with provided candidates.
	fn keygen(candidates: BTreeSet<Self::ValidatorId>);

	/// Get the current rotation status.
	fn status() -> AsyncResult<VaultStatus<Self::ValidatorId>>;

	/// Activate key/s on particular chain/s. For example, setting the new key
	/// on the contract for a smart contract chain.
	fn activate();

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(_outcome: AsyncResult<VaultStatus<Self::ValidatorId>>);
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
	fn try_initiate_claim(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError>;

	/// Performs necessary settlement once a claim has been confirmed off-chain.
	fn finalize_claim(account_id: &Self::AccountId) -> Result<(), DispatchError>;

	/// Reverts a pending claim in the case of an expiry or cancellation.
	fn revert_claim(account_id: &Self::AccountId) -> Result<(), DispatchError>;
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

/// Provides the environment data for ethereum-like chains.
pub trait EthEnvironmentProvider {
	fn token_address(asset: assets::any::Asset) -> Option<EthereumAddress>;
	fn key_manager_address() -> EthereumAddress;
	fn stake_manager_address() -> EthereumAddress;
	fn vault_address() -> EthereumAddress;
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

pub trait Slashing {
	type AccountId;
	type BlockNumber;

	/// Slashes a validator for the equivalent of some number of blocks offline.
	fn slash(validator_id: &Self::AccountId, blocks_offline: Self::BlockNumber);
}

/// Can nominate a single account.
pub trait SingleSignerNomination {
	/// The id type of signer
	type SignerId;

	/// Returns a random live signer, excluding particular provided signers. The seed value is used
	/// as a source of randomness. Returns None if no signers are live.
	fn nomination_with_seed<H: Hashable>(
		seed: H,
		exclude_ids: &[Self::SignerId],
	) -> Option<Self::SignerId>;
}

pub trait ThresholdSignerNomination {
	/// The id type of signers
	type SignerId;

	/// Returns a list of live signers where the number of signers is sufficient to author a
	/// threshold signature. The seed value is used as a source of randomness.
	fn threshold_nomination_with_seed<H: Hashable>(
		seed: H,
		epoch_index: EpochIndex,
	) -> Option<BTreeSet<Self::SignerId>>;
}

#[derive(Default, Debug, TypeInfo, Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
	Active,
	// We are currently transitioning to a new key or the key doesn't yet exist.
	#[default]
	Unavailable,
}

#[derive(Default, Debug, TypeInfo, Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct EpochKey<Key> {
	pub key: Key,
	pub epoch_index: EpochIndex,
	pub key_state: KeyState,
}

/// Provides the currently valid key for multisig ceremonies.
pub trait KeyProvider<C: ChainCrypto> {
	/// Get the chain's current agg key, the epoch index for the current key and the state of that
	/// key.
	fn current_epoch_key() -> EpochKey<C::AggKey>;

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(_key: C::AggKey) {
		unimplemented!()
	}
}

/// Api trait for pallets that need to sign things.
pub trait ThresholdSigner<C>
where
	C: ChainCrypto,
{
	type RequestId: Member + Parameter + Copy + BenchmarkValue;
	type Error: Into<DispatchError>;
	type Callback: UnfilteredDispatchable;
	type KeyId: TryInto<C::AggKey> + From<Vec<u8>>;
	type ValidatorId: Debug;

	/// Initiate a signing request and return the request id and ceremony id.
	fn request_signature(payload: C::Payload) -> (Self::RequestId, CeremonyId);

	fn request_keygen_verification_signature(
		payload: C::Payload,
		key_id: Self::KeyId,
		participants: BTreeSet<Self::ValidatorId>,
	) -> (Self::RequestId, CeremonyId);

	/// Register a callback to be dispatched when the signature is available. Can fail if the
	/// provided request_id does not exist.
	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error>;

	/// Attempt to retrieve a requested signature.
	fn signature_result(
		request_id: Self::RequestId,
	) -> AsyncResult<Result<C::ThresholdSignature, Vec<Self::ValidatorId>>>;

	/// Request a signature and register a callback for when the signature is available.
	///
	/// Since the callback is registered immediately, it should never fail.
	///
	/// Note that the `callback_generator` closure is *not* the callback. It is what *generates*
	/// the callback based on the request id.
	fn request_signature_with_callback(
		payload: C::Payload,
		callback_generator: impl FnOnce(Self::RequestId) -> Self::Callback,
	) -> (Self::RequestId, CeremonyId) {
		let (request_id, ceremony_id) = Self::request_signature(payload);
		Self::register_callback(request_id, callback_generator(request_id)).unwrap_or_else(|e| {
			log::error!(
				"Unable to register threshold signature callback. This should not be possible. Error: '{:?}'",
				e.into()
			);
		});
		(request_id, ceremony_id)
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
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) -> BroadcastId;
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
	type RuntimeCall;
	fn should_waive_fees(call: &Self::RuntimeCall, caller: &Self::AccountId) -> bool;
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

impl<A, B, C, D> QualifyNode for (A, B, C, D)
where
	A: QualifyNode<ValidatorId = B::ValidatorId>,
	B: QualifyNode,
	C: QualifyNode<ValidatorId = B::ValidatorId>,
	D: QualifyNode<ValidatorId = B::ValidatorId>,
	B::ValidatorId: Debug,
{
	type ValidatorId = A::ValidatorId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		A::is_qualified(validator_id) &&
			B::is_qualified(validator_id) &&
			C::is_qualified(validator_id) &&
			D::is_qualified(validator_id)
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

/// Allow pallets to register `Intent`s in the Ingress pallet.
pub trait IngressApi<C: Chain> {
	type AccountId;
	/// Issues an intent id and ingress address for a new liquidity deposit.
	fn register_liquidity_ingress_intent(
		lp_account: Self::AccountId,
		ingress_asset: C::ChainAsset,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError>;

	/// Issues an intent id and ingress address for a new swap.
	fn register_swap_intent(
		ingress_asset: C::ChainAsset,
		egress_asset: Asset,
		egress_address: ForeignChainAddress,
		relayer_commission_bps: u16,
		relayer_id: Self::AccountId,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError>;
}

impl<T: frame_system::Config> IngressApi<Ethereum> for T {
	type AccountId = T::AccountId;
	fn register_liquidity_ingress_intent(
		_lp_account: T::AccountId,
		_ingress_asset: assets::eth::Asset,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		Ok((0, ForeignChainAddress::Eth([0u8; 20])))
	}
	fn register_swap_intent(
		_ingress_asset: assets::eth::Asset,
		_egress_asset: Asset,
		_egress_address: ForeignChainAddress,
		_relayer_commission_bps: u16,
		_relayer_id: T::AccountId,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		Ok((0, ForeignChainAddress::Eth([0u8; 20])))
	}
}

impl<T: frame_system::Config> IngressApi<Polkadot> for T {
	type AccountId = T::AccountId;
	fn register_liquidity_ingress_intent(
		_lp_account: T::AccountId,
		_ingress_asset: assets::dot::Asset,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		Ok((0, ForeignChainAddress::Dot([0u8; 32])))
	}
	fn register_swap_intent(
		_ingress_asset: assets::dot::Asset,
		_egress_asset: Asset,
		_egress_address: ForeignChainAddress,
		_relayer_commission_bps: u16,
		_relayer_id: T::AccountId,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		Ok((0, ForeignChainAddress::Dot([0u8; 32])))
	}
}

/// Generates a deterministic ingress address for some combination of asset, chain and intent id.
pub trait AddressDerivationApi<C: Chain> {
	fn generate_address(
		ingress_asset: C::ChainAsset,
		intent_id: IntentId,
	) -> Result<C::ChainAccount, DispatchError>;
}

impl AddressDerivationApi<Ethereum> for () {
	fn generate_address(
		_ingress_asset: <Ethereum as Chain>::ChainAsset,
		_intent_id: IntentId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(Default::default())
	}
}

impl AddressDerivationApi<Polkadot> for () {
	fn generate_address(
		_ingress_asset: <Polkadot as Chain>::ChainAsset,
		_intent_id: IntentId,
	) -> Result<<Polkadot as Chain>::ChainAccount, DispatchError> {
		Ok([0u8; 32].into())
	}
}

pub trait AccountRoleRegistry<T: frame_system::Config> {
	fn register_account_role(who: &T::AccountId, role: AccountRole) -> DispatchResult;

	fn register_as_relayer(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Relayer)
	}

	fn register_as_liquidity_provider(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::LiquidityProvider)
	}

	fn register_as_validator(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Validator)
	}

	fn ensure_account_role(
		origin: T::RuntimeOrigin,
		role: AccountRole,
	) -> Result<T::AccountId, BadOrigin>;

	fn ensure_relayer(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		Self::ensure_account_role(origin, AccountRole::Relayer)
	}

	fn ensure_liquidity_provider(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		Self::ensure_account_role(origin, AccountRole::LiquidityProvider)
	}

	fn ensure_validator(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		Self::ensure_account_role(origin, AccountRole::Validator)
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn register_account(_account_id: T::AccountId, _role: AccountRole) {}
}

/// API that allows other pallets to Egress assets out of the State Chain.
pub trait EgressApi<C: Chain> {
	fn schedule_egress(
		foreign_asset: C::ChainAsset,
		amount: AssetAmount,
		egress_address: C::ChainAccount,
	) -> EgressId;
}

impl<T: frame_system::Config> EgressApi<Ethereum> for T {
	fn schedule_egress(
		_foreign_asset: assets::eth::Asset,
		_amount: AssetAmount,
		_egress_address: <Ethereum as Chain>::ChainAccount,
	) -> EgressId {
		(ForeignChain::Ethereum, 0)
	}
}

impl<T: frame_system::Config> EgressApi<Polkadot> for T {
	fn schedule_egress(
		_foreign_asset: assets::dot::Asset,
		_amount: AssetAmount,
		_egress_address: <Polkadot as Chain>::ChainAccount,
	) -> EgressId {
		(ForeignChain::Ethereum, 0)
	}
}

pub trait VaultTransitionHandler<C: ChainCrypto> {
	fn on_new_vault() {}
}

/// Provides information about current bids.
pub trait BidInfo {
	type Balance;
	/// Returns the smallest of all backup validator bids.
	fn get_min_backup_bid() -> Self::Balance;
}

pub trait VaultKeyWitnessedHandler<C: ChainAbi> {
	fn on_new_key_activated(
		new_public_key: C::AggKey,
		block_number: C::ChainBlockNumber,
		tx_id: C::TransactionId,
	) -> DispatchResultWithPostInfo;
}

pub trait BroadcastAnyChainGovKey {
	#[allow(clippy::result_unit_err)]
	fn broadcast(chain: ForeignChain, old_key: Option<Vec<u8>>, new_key: Vec<u8>)
		-> Result<(), ()>;
}

pub trait BroadcastComKey {
	type EthAddress;
	fn broadcast(new_key: Self::EthAddress);
}

/// Provides an interface to access the amount of Flip that is ready to be burned.
pub trait FlipBurnInfo {
	/// Takes the available Flip and returns it.
	fn take_flip_to_burn() -> AssetAmount;
}
