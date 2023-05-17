#![cfg_attr(not(feature = "std"), no_std)]

mod async_result;
pub mod liquidity;
pub use liquidity::*;

pub mod mocks;
pub mod offence_reporting;

use core::fmt::Debug;

pub use async_result::AsyncResult;

use cf_chains::{
	address::ForeignChainAddress, eth::Address, ApiCall, CcmDepositMetadata, Chain, ChainAbi,
	ChainCrypto, Ethereum, Polkadot,
};
use cf_primitives::{
	chains::assets, AccountRole, Asset, AssetAmount, AuthorityCount, BasisPoints, BroadcastId,
	CeremonyId, ChannelId, EgressId, EpochIndex, ForeignChain, ThresholdSignatureRequestId,
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
	DispatchError, DispatchResult, FixedPointOperand, Percent, RuntimeDebug,
};
use sp_std::{collections::btree_set::BTreeSet, iter::Sum, marker::PhantomData, prelude::*};

/// Common base config for Chainflip pallets.
pub trait Chainflip: frame_system::Config {
	/// The type used for Flip balances and auction bids.
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

	/// The overarching call type, with some added constraints.
	type RuntimeCall: Member
		+ Parameter
		+ UnfilteredDispatchable<RuntimeOrigin = Self::RuntimeOrigin>
		+ IsType<<Self as frame_system::Config>::RuntimeCall>;

	/// A type that allows us to check if a call was a result of witness consensus.
	type EnsureWitnessed: EnsureOrigin<Self::RuntimeOrigin>;
	/// A type that allows us to check if a call was a result of witness consensus by the current
	/// epoch.
	type EnsureWitnessedAtCurrentEpoch: EnsureOrigin<Self::RuntimeOrigin>;
	/// Allows us to check for the governance origin.
	type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
	/// Information about the current Epoch.
	type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
	/// Access to information about the current system state
	type SystemState: SystemStateInfo;
	/// For registering and checking account roles.
	type AccountRoleRegistry: AccountRoleRegistry<Self>;
	/// For checking nodes' current balances.
	type FundingInfo: FundingInfo<AccountId = Self::AccountId, Balance = Self::Amount>;
}

pub trait EpochInfo {
	/// The id type used for the validators.
	type ValidatorId;
	/// An amount
	type Amount;

	/// The last expired epoch
	fn last_expired_epoch() -> EpochIndex;

	/// The current authority set's validator ids
	fn current_authorities() -> BTreeSet<Self::ValidatorId>;

	/// Get the current number of authorities
	fn current_authority_count() -> AuthorityCount;

	/// Gets authority index of a particular authority for a given epoch
	fn authority_index(
		epoch_index: EpochIndex,
		account: &Self::ValidatorId,
	) -> Option<AuthorityCount>;

	/// Authority count at a particular epoch.
	fn authority_count_at_epoch(epoch: EpochIndex) -> Option<AuthorityCount>;

	/// The bond amount for this epoch. Authorities can only redeem funds above this minumum
	/// balance.
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> EpochIndex;

	/// Are we in the auction phase of the epoch?
	fn is_auction_phase() -> bool;

	#[cfg(feature = "runtime-benchmarks")]
	fn add_authority_info_for_epoch(
		epoch_index: EpochIndex,
		new_authorities: BTreeSet<Self::ValidatorId>,
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
	KeyHandoverComplete,
	RotationComplete,
	Failed(BTreeSet<ValidatorId>),
}

pub trait VaultRotator {
	type ValidatorId: Ord + Clone;

	/// Start the rotation by kicking off keygen with provided candidates.
	fn keygen(candidates: BTreeSet<Self::ValidatorId>, new_epoch_index: EpochIndex);

	/// Start the key handover with the participating candidates.
	fn key_handover(
		// Authorities of the last epoch selected to share their key in the key handover
		sharing_participants: BTreeSet<Self::ValidatorId>,
		// These are any authorities for the new epoch who are not sharing participants
		receiving_participants: BTreeSet<Self::ValidatorId>,
		epoch_index: EpochIndex,
	);

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
	/// Provide a list of validators whose accounts are in the `bidding` state.
	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>>;
}

pub trait OnAccountFunded {
	type ValidatorId;
	type Amount;

	/// A callback that is triggered after some validator's balance has changed signigicantly,
	/// either by funding it with more Flip, or by executing a redemption.
	///
	/// Note this does not trigger on small changes like transaction fees.
	///
	/// TODO: This should be triggered when funds are paid in tokenholder governance.
	fn on_account_funded(validator_id: &Self::ValidatorId, new_total: Self::Amount);
}

pub trait Funding {
	type AccountId;
	type Balance;
	type Handler: OnAccountFunded<ValidatorId = Self::AccountId, Amount = Self::Balance>;

	/// The account's total Flip balance.
	fn account_balance(account_id: &Self::AccountId) -> Self::Balance;

	/// An account's tokens that are free to be redeemed.
	fn redeemable_balance(account_id: &Self::AccountId) -> Self::Balance;

	/// Credit an account with funds from off-chain. Returns the total balance in the account after
	/// the funds are credited.
	fn credit_funds(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance;

	/// Reserves funds for a redemption, if enough redeemable funds are available.
	///
	/// Note this function makes no assumptions about how many redemptions may be pending
	/// simultaneously: if enough funds are available, it succeeds. Otherwise, it fails.
	fn try_initiate_redemption(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError>;

	/// Performs necessary settlement once a redemption has been confirmed off-chain.
	fn finalize_redemption(account_id: &Self::AccountId) -> Result<(), DispatchError>;

	/// Reverts a pending redemption in the case of an expiry or cancellation.
	fn revert_redemption(account_id: &Self::AccountId) -> Result<(), DispatchError>;
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

pub trait Slashing {
	type AccountId;
	type BlockNumber;

	/// Slashes a validator for the equivalent of some number of blocks offline.
	fn slash(validator_id: &Self::AccountId, blocks_offline: Self::BlockNumber);

	/// Slahes a percentage of a validator's total balance.
	fn slash_balance(account_id: &Self::AccountId, amount: Percent);
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

#[derive(Debug, TypeInfo, Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
	Unlocked,
	/// Key is only available to sign this request id.
	Locked(ThresholdSignatureRequestId),
}

impl KeyState {
	pub fn is_available_for_request(&self, request_id: ThresholdSignatureRequestId) -> bool {
		match self {
			KeyState::Unlocked => true,
			KeyState::Locked(locked_request_id) => request_id == *locked_request_id,
		}
	}

	pub fn unlock(&mut self) {
		*self = KeyState::Unlocked;
	}
}

#[derive(Debug, TypeInfo, Decode, Encode, Clone, Copy, PartialEq, Eq)]
pub struct EpochKey<Key> {
	pub key: Key,
	pub epoch_index: EpochIndex,
	pub key_state: KeyState,
}

impl<Key> EpochKey<Key> {
	pub fn lock_for_request(&mut self, request_id: ThresholdSignatureRequestId) {
		self.key_state = KeyState::Locked(request_id);
	}
}

/// Provides the currently valid key for multisig ceremonies.
pub trait KeyProvider<C: ChainCrypto> {
	/// Get the chain's current agg key, the epoch index for the current key and the state of that
	/// key. If no key has been set, returns None.
	fn current_epoch_key() -> Option<EpochKey<C::AggKey>>;

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
	type Error: Into<DispatchError>;
	type Callback: UnfilteredDispatchable;
	type ValidatorId: Debug;

	/// Initiate a signing request and return the request id and, if the request was successful, the
	/// ceremony id.
	fn request_signature(payload: C::Payload) -> ThresholdSignatureRequestId;

	fn request_keygen_verification_signature(
		payload: C::Payload,
		participants: BTreeSet<Self::ValidatorId>,
		key: C::AggKey,
		epoch_index: EpochIndex,
	) -> ThresholdSignatureRequestId;

	/// Register a callback to be dispatched when the signature is available. Can fail if the
	/// provided request_id does not exist.
	fn register_callback(
		request_id: ThresholdSignatureRequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error>;

	/// Attempt to retrieve a requested signature.
	fn signature_result(
		request_id: ThresholdSignatureRequestId,
	) -> AsyncResult<Result<C::ThresholdSignature, Vec<Self::ValidatorId>>>;

	/// Request a signature and register a callback for when the signature is available.
	///
	/// Since the callback is registered immediately, it should never fail.
	///
	/// Note that the `callback_generator` closure is *not* the callback. It is what *generates*
	/// the callback based on the request id.
	fn request_signature_with_callback(
		payload: C::Payload,
		callback_generator: impl FnOnce(ThresholdSignatureRequestId) -> Self::Callback,
	) -> ThresholdSignatureRequestId {
		let request_id = Self::request_signature(payload);
		Self::register_callback(request_id, callback_generator(request_id)).unwrap_or_else(|e| {
			log::error!(
				"Unable to register threshold signature callback. This should not be possible. Error: '{:?}'",
				e.into()
			);
		});
		request_id
	}

	/// Helper function to enable benchmarking of the broadcast pallet
	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(
		_request_id: ThresholdSignatureRequestId,
		_signature: C::ThresholdSignature,
	) {
		unimplemented!();
	}
}

/// Something that is capable of encoding and broadcasting native blockchain api calls to external
/// chains.
pub trait Broadcaster<Api: ChainAbi> {
	/// Supported api calls for this chain.
	type ApiCall: ApiCall<Api>;

	/// The callback that gets executed when the signature is accepted.
	type Callback: UnfilteredDispatchable;

	/// Request a threshold signature and then build and broadcast the outbound api call.
	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId);

	/// Like `threshold_sign_and_broadcast` but also registers a callback to be dispatched when the
	/// signature accepted event has been witnessed.
	fn threshold_sign_and_broadcast_with_callback(
		api_call: Self::ApiCall,
		callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId);

	fn threshold_sign_and_broadcast_for_rotation(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId);
}

pub trait BroadcastCleanup<C: Chain> {
	/// Triggers any necessary cleanup.
	fn clean_up_broadcast(broadcast_id: BroadcastId) -> DispatchResult;
}

/// The heartbeat of the network
pub trait Heartbeat {
	type ValidatorId;
	type BlockNumber;
	/// Called on every heartbeat interval
	fn on_heartbeat_interval();
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
	fn epoch_authorities(epoch: Self::EpochIndex) -> BTreeSet<Self::ValidatorId>;
	/// The bond for an epoch
	fn epoch_bond(epoch: Self::EpochIndex) -> Self::Amount;
	/// The unexpired epochs for which a node was in the authority set.
	fn active_epochs_for_authority(id: &Self::ValidatorId) -> Vec<Self::EpochIndex>;
	/// Removes an epoch from an authority's list of active epochs.
	fn deactivate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex);
	/// Add an epoch to an authority's list of active epochs.
	fn activate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex);
	/// Returns the amount of a authority's funds that are currently bonded.
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
	/// Increment the ceremony id, returning the new one.
	fn increment_ceremony_id() -> CeremonyId;
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
		unimplemented!()
	}

	/// Burns an amount of tokens, if the account has enough. Otherwise fails.
	fn try_burn_fee(account_id: &Self::AccountId, amount: Self::Amount) -> DispatchResult;
}

/// Provides information about on-chain funds.
pub trait FundingInfo {
	type AccountId;
	type Balance;
	/// Returns the funding balance of an account.
	fn total_balance_of(account_id: &Self::AccountId) -> Self::Balance;
	/// Returns the total amount of funds held on-chain.
	fn total_onchain_funds() -> Self::Balance;
}

/// Allow pallets to open and expire deposit addresses.
pub trait DepositApi<C: Chain> {
	type AccountId;

	/// Issues a channel id and deposit address for a new liquidity deposit.
	fn request_liquidity_deposit_address(
		lp_account: Self::AccountId,
		source_asset: C::ChainAsset,
	) -> Result<(ChannelId, ForeignChainAddress), DispatchError>;

	/// Issues a channel id and deposit address for a new swap.
	fn request_swap_deposit_address(
		source_asset: C::ChainAsset,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_commission_bps: BasisPoints,
		broker_id: Self::AccountId,
		message_metadata: Option<CcmDepositMetadata>,
	) -> Result<(ChannelId, ForeignChainAddress), DispatchError>;

	/// Expires a channel.
	fn expire_channel(channel_id: ChannelId, address: C::ChainAccount);
}

/// Generates a deterministic deposit address for some combination of asset, chain and channel id.
pub trait AddressDerivationApi<C: Chain> {
	fn generate_address(
		source_asset: C::ChainAsset,
		channel_id: ChannelId,
	) -> Result<C::ChainAccount, DispatchError>;
}

impl AddressDerivationApi<Ethereum> for () {
	fn generate_address(
		source_asset: <Ethereum as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(Address::from_slice(&(source_asset, channel_id).encode().blake2_256()[..20]))
	}
}

impl AddressDerivationApi<Polkadot> for () {
	fn generate_address(
		source_asset: <Polkadot as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<<Polkadot as Chain>::ChainAccount, DispatchError> {
		Ok((source_asset, channel_id).encode().blake2_256().into())
	}
}

pub trait AccountRoleRegistry<T: frame_system::Config> {
	fn register_account_role(who: &T::AccountId, role: AccountRole) -> DispatchResult;

	fn register_as_broker(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Broker)
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

	fn ensure_broker(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		Self::ensure_account_role(origin, AccountRole::Broker)
	}

	fn ensure_liquidity_provider(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		Self::ensure_account_role(origin, AccountRole::LiquidityProvider)
	}

	fn ensure_validator(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		Self::ensure_account_role(origin, AccountRole::Validator)
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn register_account(account_id: T::AccountId, role: AccountRole);

	#[cfg(feature = "runtime-benchmarks")]
	fn get_account_role(account_id: T::AccountId) -> AccountRole;
}

/// API that allows other pallets to Egress assets out of the State Chain.
pub trait EgressApi<C: Chain> {
	fn schedule_egress(
		asset: C::ChainAsset,
		amount: C::ChainAmount,
		destination_address: C::ChainAccount,
		maybe_message: Option<CcmDepositMetadata>,
	) -> EgressId;
}

impl<T: frame_system::Config> EgressApi<Ethereum> for T {
	fn schedule_egress(
		_asset: assets::eth::Asset,
		_amount: <Ethereum as Chain>::ChainAmount,
		_destination_address: <Ethereum as Chain>::ChainAccount,
		_maybe_message: Option<CcmDepositMetadata>,
	) -> EgressId {
		(ForeignChain::Ethereum, 0)
	}
}

impl<T: frame_system::Config> EgressApi<Polkadot> for T {
	fn schedule_egress(
		_asset: assets::dot::Asset,
		_amount: <Polkadot as Chain>::ChainAmount,
		_destination_address: <Polkadot as Chain>::ChainAccount,
		_maybe_message: Option<CcmDepositMetadata>,
	) -> EgressId {
		(ForeignChain::Polkadot, 0)
	}
}

pub trait VaultTransitionHandler<C: ChainCrypto> {
	fn on_new_vault() {}
}
pub trait VaultKeyWitnessedHandler<C: ChainAbi> {
	fn on_new_key_activated(block_number: C::ChainBlockNumber) -> DispatchResultWithPostInfo;
}

pub trait BroadcastAnyChainGovKey {
	#[allow(clippy::result_unit_err)]
	fn broadcast_gov_key(
		chain: ForeignChain,
		old_key: Option<Vec<u8>>,
		new_key: Vec<u8>,
	) -> Result<(), ()>;

	fn is_govkey_compatible(chain: ForeignChain, key: &[u8]) -> bool;
}

pub trait CommKeyBroadcaster {
	fn broadcast(new_key: <Ethereum as ChainCrypto>::GovKey);
}

/// Provides an interface to access the amount of Flip that is ready to be burned.
pub trait FlipBurnInfo {
	/// Takes the available Flip and returns it.
	fn take_flip_to_burn() -> AssetAmount;
}

/// The trait implementation is intentionally no-op by default
pub trait DepositHandler<C: ChainCrypto> {
	fn on_deposit_made(
		_tx_id: <C as ChainCrypto>::TransactionInId,
		_amount: <C as Chain>::ChainAmount,
		_address: <C as Chain>::ChainAccount,
		_asset: <C as Chain>::ChainAsset,
	) {
	}
	fn on_channel_opened(
		_address: <C as Chain>::ChainAccount,
		_channel_id: ChannelId,
	) -> Result<(), DispatchError> {
		Ok(())
	}
}

/// Trait for handling cross chain messages.
pub trait CcmHandler {
	/// Triggered when a ccm deposit is made.
	fn on_ccm_deposit(
		source_asset: Asset,
		deposit_amount: AssetAmount,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		message_metadata: CcmDepositMetadata,
	);
}

impl CcmHandler for () {
	fn on_ccm_deposit(
		_source_asset: Asset,
		_deposit_amount: AssetAmount,
		_destination_asset: Asset,
		_destination_address: ForeignChainAddress,
		_message_metadata: CcmDepositMetadata,
	) {
	}
}

pub trait OnRotationCallback<C: ChainCrypto> {
	type Origin;
	type Callback: UnfilteredDispatchable<RuntimeOrigin = Self::Origin>;

	fn on_rotation(
		_block_number: C::ChainBlockNumber,
		_tx_out_id: C::TransactionOutId,
	) -> Option<Self::Callback> {
		None
	}
}

pub trait OnBroadcastReady<C: ChainAbi> {
	type ApiCall: ApiCall<C>;

	fn on_broadcast_ready(_api_call: &Self::ApiCall) {}
}
