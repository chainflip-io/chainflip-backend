#![cfg_attr(not(feature = "std"), no_std)]

mod async_result;
pub mod liquidity;
use cfe_events::{KeyHandoverRequest, KeygenRequest, TxBroadcastRequest};
pub use liquidity::*;
pub mod safe_mode;
pub use safe_mode::*;

pub mod mocks;
pub mod offence_reporting;

use core::fmt::Debug;

pub use async_result::AsyncResult;

use cf_chains::{
	address::ForeignChainAddress, ApiCall, CcmChannelMetadata, CcmDepositMetadata, Chain,
	ChainCrypto, DepositChannel, Ethereum, SwapOrigin,
};
use cf_primitives::{
	AccountRole, Asset, AssetAmount, AuthorityCount, BasisPoints, BroadcastId, ChannelId,
	Ed25519PublicKey, EgressCounter, EgressId, EpochIndex, FlipBalance, ForeignChain, Ipv6Addr,
	NetworkEnvironment, SemVer, SwapId, ThresholdSignatureRequestId,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	error::BadOrigin,
	pallet_prelude::{DispatchResultWithPostInfo, Member},
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, Bounded, MaybeSerializeDeserialize},
		DispatchError, DispatchResult, FixedPointOperand, Percent, RuntimeDebug,
	},
	traits::{EnsureOrigin, Get, Imbalance, IsType, UnfilteredDispatchable},
	CloneNoBound, EqNoBound, Hashable, Parameter, PartialEqNoBound,
};
use scale_info::TypeInfo;
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
	type EnsurePrewitnessed: EnsureOrigin<Self::RuntimeOrigin>;
	/// A type that allows us to check if a call was a result of witness consensus.
	type EnsureWitnessed: EnsureOrigin<Self::RuntimeOrigin>;
	/// A type that allows us to check if a call was a result of witness consensus by the current
	/// epoch.
	type EnsureWitnessedAtCurrentEpoch: EnsureOrigin<Self::RuntimeOrigin>;
	/// Allows us to check for the governance origin.
	type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
	/// Information about the current Epoch.
	type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
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

	/// The authority set for a given epoch
	fn authorities_at_epoch(epoch: EpochIndex) -> BTreeSet<Self::ValidatorId>;

	/// Get the current number of authorities
	fn current_authority_count() -> AuthorityCount;

	/// Gets authority index of a particular authority for a given epoch
	fn authority_index(
		epoch_index: EpochIndex,
		account: &Self::ValidatorId,
	) -> Option<AuthorityCount>;

	/// Authority count at a particular epoch.
	fn authority_count_at_epoch(epoch: EpochIndex) -> Option<AuthorityCount>;

	/// The bond amount for this epoch. Authorities can only redeem funds above this minimum
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

	#[cfg(feature = "runtime-benchmarks")]
	fn set_authorities(authorities: BTreeSet<Self::ValidatorId>);
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

#[derive(PartialEq, Eq, Clone, Debug, Decode, Encode)]
pub enum KeyRotationStatusOuter<ValidatorId> {
	KeygenComplete,
	KeyHandoverComplete,
	RotationComplete,
	Failed(BTreeSet<ValidatorId>),
}

pub trait KeyRotator {
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
	fn status() -> AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>;

	/// Activate key on for vaults on all chains that use this Key.
	fn activate_keys();

	/// Reset the state associated with the current key rotation
	/// in preparation for a new one.
	fn reset_key_rotation();

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(_outcome: AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>);
}

pub trait VaultActivator<C: ChainCrypto> {
	type ValidatorId: Ord + Clone;

	/// Get the current rotation status.
	fn status() -> AsyncResult<()>;

	/// Activate key/s on particular chain/s. For example, setting the new key
	/// on the contract for a smart contract chain.
	/// Can also complete the activation if we don't require a signing ceremony
	fn start_key_activation(
		new_key: C::AggKey,
		maybe_old_key: Option<C::AggKey>,
	) -> Vec<StartKeyActivationResult>;

	/// Final step of key activation which result in the vault activation (in case we need to wait
	/// for the signing ceremony to complete)
	fn activate_key();

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(_outcome: AsyncResult<()>);
}

#[derive(Clone, Eq, PartialEq)]
pub enum StartKeyActivationResult {
	FirstVault,
	Normal(ThresholdSignatureRequestId),
	ActivationTxNotRequired,
	ActivationTxFailed,
	ChainNotInitialized,
}

/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// When an epoch has been expired.
	fn on_expired_epoch(_expired: EpochIndex) {}
}

pub trait ReputationResetter {
	type ValidatorId;

	/// Reset the reputation of a validator
	fn reset_reputation(validator: &Self::ValidatorId);
}

/// Providing bidders for an auction
pub trait BidderProvider {
	type ValidatorId: Ord;
	type Amount;
	/// Provide a list of validators whose accounts are in the `bidding` state.
	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>>;
	fn get_qualified_bidders<Q: QualifyNode<Self::ValidatorId>>(
	) -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		Self::get_bidders()
			.into_iter()
			.filter(|Bid { ref bidder_id, .. }| Q::is_qualified(bidder_id))
			.collect()
	}
	fn is_bidder(validator_id: &Self::ValidatorId) -> bool {
		Self::get_bidders()
			.iter()
			.any(|Bid { ref bidder_id, .. }| bidder_id == validator_id)
	}
}

pub trait OnAccountFunded {
	type ValidatorId;
	type Amount;

	/// A callback that is triggered after some validator's balance has changed significantly,
	/// either by funding it with more Flip, or by initiating/reverting a redemption.
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

pub trait AccountInfo<T: Chainflip> {
	/// Returns the account's total Flip balance.
	fn balance(account_id: &T::AccountId) -> T::Amount;

	/// Returns the bond on the account.
	fn bond(account_id: &T::AccountId) -> T::Amount;

	/// Returns the account's liquid funds, net of the bond.
	fn liquid_funds(account_id: &T::AccountId) -> T::Amount;
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

	/// Burn some funds that are off-chain (eg. in the StateChainGateway contract).
	///
	/// Use with care.
	fn burn_offchain(amount: Self::Balance);
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
	type Balance;

	/// Slashes a validator for the equivalent of some number of blocks offline.
	fn slash(validator_id: &Self::AccountId, blocks_offline: Self::BlockNumber);

	/// Slashes a validator by some fixed amount.
	fn slash_balance(account_id: &Self::AccountId, slash_amount: FlipBalance);

	/// Calculate the amount of FLIP to slash
	fn calculate_slash_amount(
		account_id: &Self::AccountId,
		blocks: Self::BlockNumber,
	) -> Self::Balance;
}

/// Nominate a single account for transaction broadcasting.
pub trait BroadcastNomination {
	/// The id type of the broadcaster.
	type BroadcasterId;

	/// Returns a random broadcaster id, excluding particular provided ids. The seed value is used
	/// as a source of randomness. Returns None if no signers are live.
	fn nominate_broadcaster<H: Hashable>(
		seed: H,
		exclude_ids: impl IntoIterator<Item = Self::BroadcasterId>,
	) -> Option<Self::BroadcasterId>;
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
pub struct EpochKey<Key> {
	pub key: Key,
	pub epoch_index: EpochIndex,
}

/// Provides the currently valid key for multisig ceremonies.
pub trait KeyProvider<C: ChainCrypto> {
	/// Get the chain's active agg key, key state and associated epoch index. If no key is active,
	/// returns None.
	///
	/// Note that the epoch may not be the current epoch: a key can be activated before the start of
	/// the epoch.
	fn active_epoch_key() -> Option<EpochKey<C::AggKey>>;

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(_key: C::AggKey, _epoch: EpochIndex) {
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

pub trait CfeMultisigRequest<T: Chainflip, C: ChainCrypto> {
	fn keygen_request(req: KeygenRequest<T::ValidatorId>);

	fn signature_request(req: cfe_events::ThresholdSignatureRequest<T::ValidatorId, C>);

	fn key_handover_request(_req: KeyHandoverRequest<T::ValidatorId, C>) {
		assert!(!C::key_handover_is_required());
	}
}

pub trait CfePeerRegistration<T: Chainflip> {
	fn peer_registered(
		account_id: T::ValidatorId,
		pubkey: Ed25519PublicKey,
		port: u16,
		ip: Ipv6Addr,
	);

	fn peer_deregistered(account_id: T::ValidatorId, pubkey: Ed25519PublicKey);
}

pub trait CfeBroadcastRequest<T: Chainflip, C: Chain> {
	fn tx_broadcast_request(req: TxBroadcastRequest<T::ValidatorId, C>);
}

/// Something that is capable of encoding and broadcasting native blockchain api calls to external
/// chains.
pub trait Broadcaster<C: Chain> {
	/// Supported api calls for this chain.
	type ApiCall: ApiCall<C::ChainCrypto>;

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
		success_callback: Option<Self::Callback>,
		failed_callback_generator: impl FnOnce(BroadcastId) -> Option<Self::Callback>,
	) -> BroadcastId;

	/// Request a threshold signature and then build and broadcast the outbound api call
	/// specifically for a rotation tx..
	fn threshold_sign_and_broadcast_rotation_tx(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId);

	/// Resign a call, and update the signature data storage, but do not broadcast.
	fn threshold_resign(broadcast_id: BroadcastId) -> Option<ThresholdSignatureRequestId>;

	/// Request a call to be threshold signed, but do not broadcast.
	/// The caller must manage storage cleanup, so signatures are not stored indefinitely.
	fn threshold_sign(api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId);

	/// Clean up storage data related to a broadcast ID.
	fn clean_up_broadcast_storage(broadcast_id: BroadcastId);
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

/// Emits an event when backup rewards are distributed that lives inside the Emissions pallet.
pub trait BackupRewardsNotifier {
	type Balance;
	type AccountId;
	fn emit_event(account_id: &Self::AccountId, amount: Self::Balance);
}

/// Checks if the caller can execute free transactions
pub trait WaivedFees {
	type AccountId;
	type RuntimeCall;
	fn should_waive_fees(call: &Self::RuntimeCall, caller: &Self::AccountId) -> bool;
}

/// Qualify what is considered as a potential authority for the network
pub trait QualifyNode<Id: Ord> {
	/// Is the node qualified to be an authority and meet our expectations of one
	fn is_qualified(validator_id: &Id) -> bool;

	/// Filter out the unqualified nodes from a list of potential nodes.
	fn filter_unqualified(validators: BTreeSet<Id>) -> BTreeSet<Id> {
		validators.into_iter().filter(|v| !Self::is_qualified(v)).collect()
	}
}

/// Qualify if the node has registered
pub struct SessionKeysRegistered<T, R>((PhantomData<T>, PhantomData<R>));

impl<T: Chainflip, R: frame_support::traits::ValidatorRegistration<T::ValidatorId>>
	QualifyNode<T::ValidatorId> for SessionKeysRegistered<T, R>
{
	fn is_qualified(validator_id: &T::ValidatorId) -> bool {
		R::is_registered(validator_id)
	}
}

impl<Id: Ord, A, B> QualifyNode<Id> for (A, B)
where
	A: QualifyNode<Id>,
	B: QualifyNode<Id>,
{
	fn is_qualified(validator_id: &Id) -> bool {
		A::is_qualified(validator_id) && B::is_qualified(validator_id)
	}

	fn filter_unqualified(validators: BTreeSet<Id>) -> BTreeSet<Id> {
		B::filter_unqualified(A::filter_unqualified(validators))
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

/// Something that is able to provide block authorship slots that were missed.
pub trait MissedAuthorshipSlots {
	/// Get a list of slots that were missed.
	fn missed_slots() -> sp_std::ops::Range<u64>;
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
	type Amount;

	/// Issues a channel id and deposit address for a new liquidity deposit.
	fn request_liquidity_deposit_address(
		lp_account: Self::AccountId,
		source_asset: C::ChainAsset,
		boost_fee: BasisPoints,
	) -> Result<(ChannelId, ForeignChainAddress, C::ChainBlockNumber, Self::Amount), DispatchError>;

	/// Issues a channel id and deposit address for a new swap.
	fn request_swap_deposit_address(
		source_asset: C::ChainAsset,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_commission_bps: BasisPoints,
		broker_id: Self::AccountId,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: BasisPoints,
	) -> Result<(ChannelId, ForeignChainAddress, C::ChainBlockNumber, Self::Amount), DispatchError>;
}

pub trait AccountRoleRegistry<T: frame_system::Config> {
	fn register_account_role(who: &T::AccountId, role: AccountRole) -> DispatchResult;

	fn deregister_account_role(who: &T::AccountId, role: AccountRole) -> DispatchResult;

	fn has_account_role(who: &T::AccountId, role: AccountRole) -> bool;

	fn is_unregistered(who: &T::AccountId) -> bool {
		Self::has_account_role(who, AccountRole::Unregistered)
	}

	fn register_as_broker(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Broker)
	}

	fn register_as_liquidity_provider(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::LiquidityProvider)
	}

	fn register_as_validator(account_id: &T::AccountId) -> DispatchResult {
		Self::register_account_role(account_id, AccountRole::Validator)
	}

	fn deregister_as_broker(account_id: &T::AccountId) -> DispatchResult {
		Self::deregister_account_role(account_id, AccountRole::Broker)
	}

	fn deregister_as_liquidity_provider(account_id: &T::AccountId) -> DispatchResult {
		Self::deregister_account_role(account_id, AccountRole::LiquidityProvider)
	}

	fn deregister_as_validator(account_id: &T::AccountId) -> DispatchResult {
		Self::deregister_account_role(account_id, AccountRole::Validator)
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
	fn whitelisted_caller_with_role(role: AccountRole) -> Result<T::AccountId, DispatchError> {
		use frame_support::traits::OnNewAccount;
		let caller = frame_benchmarking::whitelisted_caller::<T::AccountId>();
		if frame_system::Pallet::<T>::providers(&caller) == 0u32 {
			frame_system::Pallet::<T>::inc_providers(&caller);
		}
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		Self::register_account_role(&caller, role)?;
		Ok(caller.clone())
	}
}

#[derive(
	PartialEqNoBound, EqNoBound, CloneNoBound, Encode, Decode, TypeInfo, MaxEncodedLen, RuntimeDebug,
)]
pub struct ScheduledEgressDetails<C: Chain> {
	pub egress_id: EgressId,
	pub egress_amount: C::ChainAmount,
	pub fee_withheld: C::ChainAmount,
}

impl<C: Chain + Get<ForeignChain>> Default for ScheduledEgressDetails<C> {
	fn default() -> Self {
		Self::new(Default::default(), Default::default(), Default::default())
	}
}

impl<C: Chain + Get<ForeignChain>> ScheduledEgressDetails<C> {
	pub fn new(
		id_counter: EgressCounter,
		egress_amount: C::ChainAmount,
		fee_withheld: C::ChainAmount,
	) -> Self {
		Self {
			egress_id: (<C as Get<ForeignChain>>::get(), id_counter),
			egress_amount,
			fee_withheld,
		}
	}
}

/// API that allows other pallets to Egress assets out of the State Chain.
pub trait EgressApi<C: Chain> {
	type EgressError: Into<DispatchError>;

	/// Schedule the egress of an asset to a destination address.
	///
	/// May take a fee and will return an error if egress cannot be scheduled.
	fn schedule_egress(
		asset: C::ChainAsset,
		amount: C::ChainAmount,
		destination_address: C::ChainAccount,
		maybe_ccm_with_gas_budget: Option<(CcmDepositMetadata, C::ChainAmount)>,
	) -> Result<ScheduledEgressDetails<C>, Self::EgressError>;
}

impl<C: Chain + Get<ForeignChain>> EgressApi<C> for () {
	type EgressError = DispatchError;

	fn schedule_egress(
		_asset: C::ChainAsset,
		_amount: <C as Chain>::ChainAmount,
		_destination_address: <C as Chain>::ChainAccount,
		_maybe_ccm_with_gas_budget: Option<(CcmDepositMetadata, <C as Chain>::ChainAmount)>,
	) -> Result<ScheduledEgressDetails<C>, DispatchError> {
		Ok(Default::default())
	}
}

pub trait VaultKeyWitnessedHandler<C: Chain> {
	fn on_first_key_activated(block_number: C::ChainBlockNumber) -> DispatchResultWithPostInfo;
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
	fn broadcast(new_key: <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::GovKey);
}

/// Provides an interface to access the amount of Flip that is ready to be burned.
pub trait FlipBurnInfo {
	/// Takes the available Flip and returns it.
	fn take_flip_to_burn() -> AssetAmount;
}

/// The trait implementation is intentionally no-op by default
pub trait OnDeposit<C: Chain> {
	fn on_deposit_made(
		_deposit_details: C::DepositDetails,
		_amount: C::ChainAmount,
		_channel: DepositChannel<C>,
	) {
	}
}

pub trait NetworkEnvironmentProvider {
	fn get_network_environment() -> NetworkEnvironment;
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct CcmSwapIds {
	pub principal_swap_id: Option<SwapId>,
	pub gas_swap_id: Option<SwapId>,
}

/// Trait for handling cross chain messages.
pub trait CcmHandler {
	/// Triggered when a ccm deposit is made.
	#[allow(clippy::result_unit_err)]
	fn on_ccm_deposit(
		source_asset: Asset,
		deposit_amount: AssetAmount,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		deposit_metadata: CcmDepositMetadata,
		origin: SwapOrigin,
	) -> Result<CcmSwapIds, ()>;
}

impl CcmHandler for () {
	fn on_ccm_deposit(
		_source_asset: Asset,
		_deposit_amount: AssetAmount,
		_destination_asset: Asset,
		_destination_address: ForeignChainAddress,
		_deposit_metadata: CcmDepositMetadata,
		_origin: SwapOrigin,
	) -> Result<CcmSwapIds, ()> {
		Err(())
	}
}

pub trait OnBroadcastReady<C: Chain> {
	type ApiCall: ApiCall<C::ChainCrypto>;

	fn on_broadcast_ready(_api_call: &Self::ApiCall) {}
}

pub trait GetBitcoinFeeInfo {
	fn bitcoin_fee_info() -> cf_chains::btc::BitcoinFeeInfo;
}

pub trait GetBlockHeight<C: Chain> {
	fn get_block_height() -> C::ChainBlockNumber;
}

pub trait CompatibleCfeVersions {
	fn current_release_version() -> SemVer;
}

pub trait AuthoritiesCfeVersions {
	/// Returns the percentage of current authorities with their CFEs at the given version.
	fn percent_authorities_compatible_with_version(version: SemVer) -> Percent;
}

pub trait AdjustedFeeEstimationApi<C: Chain> {
	fn estimate_ingress_fee(asset: C::ChainAsset) -> C::ChainAmount;

	fn estimate_egress_fee(asset: C::ChainAsset) -> C::ChainAmount;
}

pub trait CallDispatchFilter<RuntimeCall> {
	fn should_dispatch(&self, call: &RuntimeCall) -> bool;
}

impl<RuntimeCall> CallDispatchFilter<RuntimeCall> for () {
	fn should_dispatch(&self, _call: &RuntimeCall) -> bool {
		true
	}
}

pub trait AssetConverter {
	fn estimate_swap_input_for_desired_output<
		Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy,
	>(
		input_asset: impl Into<Asset>,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<Amount>;

	fn calculate_asset_conversion<Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy>(
		input_asset: impl Into<Asset>,
		available_input_amount: Amount,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<Amount>;
}

pub trait IngressEgressFeeApi<C: Chain> {
	fn accrue_withheld_fee(asset: C::ChainAsset, amount: C::ChainAmount);
}
