#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "std", feature(option_get_or_insert_default))]

mod async_result;
mod liquidity;
use cfe_events::{KeyHandoverRequest, KeygenRequest, TxBroadcastRequest};
pub use liquidity::*;
pub mod safe_mode;
pub use safe_mode::*;
mod swapping;

pub use swapping::{SwapRequestHandler, SwapRequestType, SwapRequestTypeEncoded, SwapType};

pub mod mocks;
pub mod offence_reporting;

use core::fmt::Debug;

pub use async_result::AsyncResult;

use cf_chains::{
	address::ForeignChainAddress,
	assets::any::AssetMap,
	sol::{SolAddress, SolHash},
	ApiCall, CcmChannelMetadata, CcmDepositMetadata, Chain, ChainCrypto, ChannelRefundParameters,
	DepositChannel, Ethereum,
};
use cf_primitives::{
	AccountRole, Asset, AssetAmount, AuthorityCount, BasisPoints, Beneficiaries, BlockNumber,
	BroadcastId, ChannelId, DcaParameters, Ed25519PublicKey, EgressCounter, EgressId, EpochIndex,
	FlipBalance, ForeignChain, Ipv6Addr, NetworkEnvironment, SemVer, ThresholdSignatureRequestId,
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
	type FundingInfo: FundingInfo<AccountId = Self::AccountId, Balance = Self::Amount>
		+ AccountInfo<AccountId = Self::AccountId, Amount = Self::Amount>;
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

	/// The authority set for a given epoch
	fn authorities_at_epoch(epoch: EpochIndex) -> Vec<Self::ValidatorId>;

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

	#[cfg(feature = "runtime-benchmarks")]
	fn add_authority_info_for_epoch(
		epoch_index: EpochIndex,
		new_authorities: Vec<Self::ValidatorId>,
	);

	#[cfg(feature = "runtime-benchmarks")]
	fn set_authorities(authorities: Vec<Self::ValidatorId>);
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
	/// When a new epoch has started.
	fn on_new_epoch(_new: EpochIndex) {}
}

pub trait ReputationResetter {
	type ValidatorId;

	/// Reset the reputation of a validator
	fn reset_reputation(validator: &Self::ValidatorId);
}

pub trait RedemptionCheck {
	type ValidatorId;
	fn ensure_can_redeem(validator_id: &Self::ValidatorId) -> DispatchResult;
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

pub trait AccountInfo {
	type AccountId;
	type Amount;
	/// Returns the account's total Flip balance.
	fn balance(account_id: &Self::AccountId) -> Self::Amount;

	/// Returns the bond on the account.
	fn bond(account_id: &Self::AccountId) -> Self::Amount;

	/// Returns the account's liquid funds, net of the bond.
	fn liquid_funds(account_id: &Self::AccountId) -> Self::Amount;
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
	#[allow(clippy::type_complexity)]
	fn signature_result(
		request_id: ThresholdSignatureRequestId,
	) -> (C::AggKey, AsyncResult<Result<C::ThresholdSignature, Vec<Self::ValidatorId>>>);

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
		_signer: C::AggKey,
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
		new_key: <<C as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> (BroadcastId, ThresholdSignatureRequestId);

	/// Request a new threshold signature for a previously aborted broadcast's payload, optionally
	/// also requesting the validators to send the transaction.
	fn re_sign_broadcast(
		broadcast_id: BroadcastId,
		request_broadcast: bool,
		refresh_replay_protection: bool,
	) -> Result<ThresholdSignatureRequestId, DispatchError>;

	/// Request a call to be threshold signed, but do not broadcast.
	/// The caller must manage storage cleanup, so signatures are not stored indefinitely.
	fn threshold_sign(api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId);

	/// Removes all data associated with a broadcast.
	fn expire_broadcast(broadcast_id: BroadcastId);
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
///
/// Note that when implementing this, it is sufficient to implement is_qualified. However, if
/// there is a high fixed cost to check if a single node is qualified (for example if we need to
/// compute some precondition or criterion), it is recommended to implement take_qualified as well.
pub trait QualifyNode<Id: Ord + Clone> {
	/// Is the node qualified to be an authority and meet our expectations of one
	fn is_qualified(validator_id: &Id) -> bool;

	/// Filter out the unqualified nodes from a list of potential nodes.
	fn filter_qualified(validators: BTreeSet<Id>) -> BTreeSet<Id> {
		validators.into_iter().filter(|v| Self::is_qualified(v)).collect()
	}

	/// Takes a vector of items and a id-selector function, and returns another vector containing
	/// only the items whose id is qualified.
	fn filter_qualified_by_key<T>(items: Vec<T>, f: impl Fn(&T) -> &Id) -> Vec<T> {
		let qualified = Self::filter_qualified(items.iter().map(&f).cloned().collect());
		items.into_iter().filter(|i| qualified.contains(f(i))).collect()
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

impl<Id: Ord + Clone, A, B> QualifyNode<Id> for (A, B)
where
	A: QualifyNode<Id>,
	B: QualifyNode<Id>,
{
	fn is_qualified(validator_id: &Id) -> bool {
		A::is_qualified(validator_id) && B::is_qualified(validator_id)
	}

	fn filter_qualified(validators: BTreeSet<Id>) -> BTreeSet<Id> {
		B::filter_qualified(A::filter_qualified(validators))
	}
}

/// Handles the check of execution conditions
pub trait ExecutionCondition {
	/// Returns true/false if the condition is satisfied
	fn is_satisfied() -> bool;
}

impl<A, B> ExecutionCondition for (A, B)
where
	A: ExecutionCondition,
	B: ExecutionCondition,
{
	fn is_satisfied() -> bool {
		A::is_satisfied() && B::is_satisfied()
	}
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
		refund_address: Option<ForeignChainAddress>,
	) -> Result<(ChannelId, ForeignChainAddress, C::ChainBlockNumber, Self::Amount), DispatchError>;

	/// Issues a channel id and deposit address for a new swap.
	fn request_swap_deposit_address(
		source_asset: C::ChainAsset,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_commission: Beneficiaries<Self::AccountId>,
		broker_id: Self::AccountId,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: BasisPoints,
		refund_params: Option<ChannelRefundParameters>,
		dca_params: Option<DcaParameters>,
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
		Self::generate_whitelisted_callers_with_role(role, 1u32).map(|r|
			// Guaranteed to return a vec with length of 1
			r[0].clone())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn generate_whitelisted_callers_with_role(
		role: AccountRole,
		num: u32,
	) -> Result<Vec<T::AccountId>, DispatchError> {
		use frame_support::traits::OnNewAccount;
		(0..num)
			.map(|n| {
				let caller =
					frame_benchmarking::account::<T::AccountId>("whitelisted_caller", n, 0);
				if frame_system::Pallet::<T>::providers(&caller) == 0u32 {
					frame_system::Pallet::<T>::inc_providers(&caller);
				}
				<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
				Self::register_account_role(&caller, role)?;
				Ok(caller)
			})
			.collect::<Result<Vec<_>, DispatchError>>()
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

pub trait VaultKeyWitnessedHandler<C: Chain> {
	fn on_first_key_activated(block_number: C::ChainBlockNumber) -> DispatchResult;
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
		_channel: &DepositChannel<C>,
	) {
	}
}

pub trait NetworkEnvironmentProvider {
	fn get_network_environment() -> NetworkEnvironment;
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
	/// Calculate the amount of an asset that is required to pay for a given amount of gas.
	///
	/// Use this for transaction fees only.
	fn calculate_input_for_gas_output<C: Chain>(
		input_asset: C::ChainAsset,
		required_gas: C::ChainAmount,
	) -> Option<C::ChainAmount>;
}

pub trait IngressEgressFeeApi<C: Chain> {
	fn accrue_withheld_fee(asset: C::ChainAsset, amount: C::ChainAmount);
}

pub trait LiabilityTracker {
	fn record_liability(account_id: ForeignChainAddress, asset: Asset, amount: AssetAmount);
}

pub trait AssetWithholding {
	fn withhold_assets(asset: Asset, amount: AssetAmount);
}

pub trait FetchesTransfersLimitProvider {
	fn maybe_transfers_limit() -> Option<usize> {
		None
	}

	fn maybe_ccm_limit() -> Option<usize> {
		None
	}

	fn maybe_fetches_limit() -> Option<usize> {
		None
	}
}

pub struct NoLimit;
impl FetchesTransfersLimitProvider for NoLimit {}

#[derive(Encode, Decode, TypeInfo)]
pub struct SwapLimits {
	pub max_swap_retry_duration_blocks: BlockNumber,
	pub max_swap_request_duration_blocks: BlockNumber,
}
pub trait SwapLimitsProvider {
	fn get_swap_limits() -> SwapLimits;
}

/// API for interacting with the asset-balance pallet.
pub trait BalanceApi {
	type AccountId;

	/// Attempt to credit the account with the given asset and amount.
	fn try_credit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;

	/// Attempt to debit the account with the given asset and amount.
	fn try_debit_account(
		who: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult;

	/// Returns the asset free balances of the given account.
	fn free_balances(who: &Self::AccountId) -> AssetMap<AssetAmount>;

	/// Returns the balance of the given account for the given asset.
	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount;
}

pub trait IngressSink {
	type Account: Member + Parameter;
	type Asset: Member + Parameter + Copy;
	type Amount: Member + Parameter + Copy + AtLeast32BitUnsigned;
	type BlockNumber: Member + Parameter + Copy + AtLeast32BitUnsigned;
	type DepositDetails;

	fn on_ingress(
		channel: Self::Account,
		asset: Self::Asset,
		amount: Self::Amount,
		block_number: Self::BlockNumber,
		details: Self::DepositDetails,
	);
	fn on_ingress_reverted(channel: Self::Account, asset: Self::Asset, amount: Self::Amount);
	fn on_channel_closed(channel: Self::Account);
}

pub trait IngressSource {
	type Chain: Chain;

	fn open_channel(
		channel: <Self::Chain as Chain>::ChainAccount,
		asset: <Self::Chain as Chain>::ChainAsset,
		close_block: <Self::Chain as Chain>::ChainBlockNumber,
	) -> DispatchResult;
}
pub struct DummyIngressSource<TargetChain: Chain> {
	_phantom: core::marker::PhantomData<TargetChain>,
}
impl<TargetChain: Chain> IngressSource for DummyIngressSource<TargetChain> {
	type Chain = TargetChain;

	fn open_channel(
		_channel: <Self::Chain as Chain>::ChainAccount,
		_asset: <Self::Chain as Chain>::ChainAsset,
		_close_block: <Self::Chain as Chain>::ChainBlockNumber,
	) -> DispatchResult {
		Ok(())
	}
}

pub trait SolanaNonceWatch {
	fn watch_for_nonce_change(
		nonce_account: SolAddress,
		previous_nonce_value: SolHash,
	) -> DispatchResult;
}

impl SolanaNonceWatch for () {
	fn watch_for_nonce_change(
		_nonce_account: SolAddress,
		_previous_nonce_value: SolHash,
	) -> DispatchResult {
		Ok(())
	}
}

pub trait ElectionEgressWitnesser {
	type Chain: ChainCrypto;

	fn watch_for_egress_success(
		tx_out_id: <Self::Chain as ChainCrypto>::TransactionOutId,
	) -> DispatchResult;
}

pub struct DummyEgressSuccessWitnesser<C> {
	_phantom: core::marker::PhantomData<C>,
}

impl<C: ChainCrypto> ElectionEgressWitnesser for DummyEgressSuccessWitnesser<C> {
	type Chain = C;

	fn watch_for_egress_success(
		_tx_out_id: <Self::Chain as ChainCrypto>::TransactionOutId,
	) -> DispatchResult {
		Ok(())
	}
}

/// This trait is used by the validator pallet to check if a rotation tx is still pending for any of
/// the chains. This is needed by the validator pallet to decide whether to start a new rotation.
pub trait RotationBroadcastsPending {
	fn rotation_broadcasts_pending() -> bool;
}
