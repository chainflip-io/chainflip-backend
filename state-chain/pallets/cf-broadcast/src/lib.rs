#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#![feature(extract_if)]
#![feature(is_sorted)]

mod benchmarking;
mod mock;
mod tests;

pub mod migrations;
pub mod weights;
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
use cf_traits::{GetBlockHeight, SafeMode};
use frame_support::{traits::OriginTrait, RuntimeDebug};
use sp_std::marker;
pub use weights::WeightInfo;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
#[scale_info(skip_type_params(I))]
pub struct PalletSafeMode<I: 'static> {
	pub retry_enabled: bool,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: marker::PhantomData<I>,
}

impl<I: 'static> SafeMode for PalletSafeMode<I> {
	const CODE_RED: Self = PalletSafeMode { retry_enabled: false, _phantom: marker::PhantomData };
	const CODE_GREEN: Self = PalletSafeMode { retry_enabled: true, _phantom: marker::PhantomData };
}

use cf_chains::{
	ApiCall, Chain, ChainCrypto, FeeRefundCalculator, TransactionBuilder, TransactionMetadata as _,
};
use cf_traits::{
	offence_reporting::OffenceReporter, BroadcastNomination, Broadcaster, Chainflip, EpochInfo,
	EpochKey, KeyProvider, ThresholdSigner,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	pallet_prelude::DispatchResult,
	sp_runtime::traits::Saturating,
	traits::{Get, StorageVersion, UnfilteredDispatchable},
	Twox64Concat,
};
use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_std::{marker::PhantomData, prelude::*};

/// The number of broadcast attempts that were made before this one.
pub type AttemptCount = u32;

/// A unique id for each broadcast attempt
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default, Copy)]
pub struct BroadcastAttemptId {
	pub broadcast_id: BroadcastId,
	pub attempt_count: AttemptCount,
}

impl BroadcastAttemptId {
	/// Get the next BroadcastAttemptId.
	pub fn peek_next(&self) -> Self {
		Self { attempt_count: self.attempt_count + 1, ..*self }
	}

	/// Increment the attempt counter and return the next BroadcastAttemptId.
	pub fn into_next<T: Config<I>, I: 'static>(self) -> Self {
		Self {
			attempt_count: BroadcastAttemptCount::<T, I>::mutate(
				self.broadcast_id,
				|attempt_count: &mut AttemptCount| {
					*attempt_count += 1;
					*attempt_count
				},
			),
			..self
		}
	}
}

impl sp_std::fmt::Display for BroadcastAttemptId {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		write!(
			f,
			"BroadcastAttemptId(broadcast_id: {}, attempt_count: {})",
			self.broadcast_id, self.attempt_count
		)
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedToBroadcastTransaction,
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::benchmarking_value::BenchmarkValue;
	use cf_traits::{AccountRoleRegistry, BroadcastNomination, KeyProvider, OnBroadcastReady};
	use frame_support::{
		ensure,
		pallet_prelude::{OptionQuery, ValueQuery, *},
		traits::EnsureOrigin,
	};
	use frame_system::pallet_prelude::*;

	/// Type alias for the instance's configured Transaction.
	pub type TransactionFor<T, I> = <<T as Config<I>>::TargetChain as Chain>::Transaction;

	/// Type alias for the instance's configured SignerId.
	pub type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	/// Type alias for the threshold signature
	pub type ThresholdSignatureFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature;

	pub type TransactionOutIdFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;

	/// Type alias for the instance's configured Payload.
	pub type PayloadFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::Payload;

	/// Type alias for the instance's configured transaction Metadata.
	pub type TransactionMetadataFor<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::TransactionMetadata;

	pub type ChainBlockNumberFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainBlockNumber;

	/// Type alias for the Amount type of a particular chain.
	pub type ChainAmountFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainAmount;

	/// Type alias for the Amount type of a particular chain.
	pub type TransactionFeeFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::TransactionFee;

	/// Type alias for the instance's configured ApiCall.
	pub type ApiCallFor<T, I> = <T as Config<I>>::ApiCall;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct BroadcastAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt_id: BroadcastAttemptId,
		pub transaction_payload: TransactionFor<T, I>,
		pub threshold_signature_payload: PayloadFor<T, I>,
		pub transaction_out_id: TransactionOutIdFor<T, I>,
	}

	/// The first step in the process - a transaction signing attempt.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt: BroadcastAttempt<T, I>,
		pub nominee: T::ValidatorId,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// The top-level origin type of the runtime.
		type RuntimeOrigin: From<Origin<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<Result<Origin<Self, I>, <Self as Config<I>>::RuntimeOrigin>>;

		/// The call type that is used to dispatch a broadcast callback.
		type BroadcastCallable: Member
			+ Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>;

		/// Offences that can be reported in this runtime.
		type Offence: From<PalletOffence>;

		/// A marker trait identifying the chain that we are broadcasting to.
		type TargetChain: Chain;

		/// The api calls supported by this broadcaster.
		type ApiCall: ApiCall<<<Self as pallet::Config<I>>::TargetChain as Chain>::ChainCrypto>
			+ BenchmarkValue
			+ Send
			+ Sync;

		/// Builds the transaction according to the chain's environment settings.
		type TransactionBuilder: TransactionBuilder<Self::TargetChain, Self::ApiCall>;

		/// A threshold signer that can sign calls for this chain, and dispatch callbacks into this
		/// pallet.
		type ThresholdSigner: ThresholdSigner<
			<Self::TargetChain as Chain>::ChainCrypto,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Signer nomination.
		type BroadcastSignerNomination: BroadcastNomination<BroadcasterId = Self::ValidatorId>;

		/// For reporting bad actors.
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ensure that only threshold signature consensus can trigger a broadcast.
		type EnsureThresholdSigned: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;

		type BroadcastReadyProvider: OnBroadcastReady<Self::TargetChain, ApiCall = Self::ApiCall>;

		/// Get the latest block height of the target chain via Chain Tracking.
		type ChainTracking: GetBlockHeight<Self::TargetChain>;

		/// The timeout duration for the broadcast, measured in number of blocks.
		#[pallet::constant]
		type BroadcastTimeout: Get<BlockNumberFor<Self>>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<<Self::TargetChain as Chain>::ChainCrypto>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;

		/// The save mode block margin
		type SafeModeBlockMargin: Get<BlockNumberFor<Self>>;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Copy, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T, I))]
	pub struct Origin<T: Config<I>, I: 'static = ()>(pub(super) PhantomData<(T, I)>);

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter for incrementing the broadcast id.
	#[pallet::storage]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	/// Callbacks to be dispatched when the SignatureAccepted event has been witnessed.
	#[pallet::storage]
	#[pallet::getter(fn request_success_callback)]
	pub type RequestSuccessCallbacks<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;

	/// Callbacks to be dispatched when a broadcast failure has been witnessed.
	#[pallet::storage]
	#[pallet::getter(fn request_failed_callback)]
	pub type RequestFailureCallbacks<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;

	/// The last attempt number for a particular broadcast.
	#[pallet::storage]
	pub type BroadcastAttemptCount<T, I = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, AttemptCount, ValueQuery>;

	/// Contains a list of the authorities that have failed to sign a particular broadcast.
	#[pallet::storage]
	pub type FailedBroadcasters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, Vec<T::ValidatorId>>;

	/// Live transaction broadcast requests.
	#[pallet::storage]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastAttemptId,
		TransactionSigningAttempt<T, I>,
		OptionQuery,
	>;

	/// Lookup table between TransactionOutId -> Broadcast.
	/// This storage item is used by the CFE to track which broadcasts/egresses it needs to
	/// witness.
	#[pallet::storage]
	pub type TransactionOutIdToBroadcastId<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TransactionOutIdFor<T, I>,
		(BroadcastId, ChainBlockNumberFor<T, I>),
		OptionQuery,
	>;

	/// The list of failed broadcasts pending retry.
	#[pallet::storage]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<BroadcastAttempt<T, I>>, ValueQuery>;

	/// A mapping from block number to a list of signing or broadcast attempts that expire at that
	/// block number.
	#[pallet::storage]
	pub type Timeouts<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<BroadcastAttemptId>, ValueQuery>;

	/// Stores all needed information to be able to re-request the signature
	#[pallet::storage]
	#[pallet::getter(fn threshold_signature_data)]
	pub type ThresholdSignatureData<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastId,
		(ApiCallFor<T, I>, ThresholdSignatureFor<T, I>),
		OptionQuery,
	>;

	/// Stores metadata related to a transaction.
	#[pallet::storage]
	pub type TransactionMetadata<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, TransactionMetadataFor<T, I>>;

	/// Tracks how much a signer id is owed for paying transaction fees.
	#[pallet::storage]
	pub type TransactionFeeDeficit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, SignerIdFor<T, I>, ChainAmountFor<T, I>, ValueQuery>;

	/// Whether or not broadcasts are paused for broadcast ids greater than the given broadcast id.
	#[pallet::storage]
	#[pallet::getter(fn broadcast_barriers)]
	pub type BroadcastBarriers<T, I = ()> = StorageValue<_, VecDeque<BroadcastId>, ValueQuery>;

	/// List of broadcasts that are initiated but not witnessed on the external chain.
	#[pallet::storage]
	#[pallet::getter(fn pending_broadcasts)]
	pub type PendingBroadcasts<T, I = ()> = StorageValue<_, Vec<BroadcastId>, ValueQuery>;

	/// List of broadcasts that have been aborted because they were unsuccessful to be broadcast
	/// after many retries.
	#[pallet::storage]
	#[pallet::getter(fn aborted_broadcasts)]
	pub type AbortedBroadcasts<T, I = ()> = StorageValue<_, Vec<BroadcastId>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A request to a specific authority to sign a transaction.
		TransactionBroadcastRequest {
			broadcast_attempt_id: BroadcastAttemptId,
			nominee: T::ValidatorId,
			transaction_payload: TransactionFor<T, I>,
			transaction_out_id: TransactionOutIdFor<T, I>,
		},
		/// A failed broadcast attempt has been scheduled for retry.
		BroadcastRetryScheduled { broadcast_attempt_id: BroadcastAttemptId },
		/// A broadcast attempt timed out.
		BroadcastAttemptTimeout { broadcast_attempt_id: BroadcastAttemptId },
		/// A broadcast has been aborted after all authorities have attempted to broadcast the
		/// transaction and failed.
		BroadcastAborted { broadcast_id: BroadcastId },
		/// A broadcast has successfully been completed.
		BroadcastSuccess {
			broadcast_id: BroadcastId,
			transaction_out_id: TransactionOutIdFor<T, I>,
		},
		/// A broadcast's threshold signature is invalid, we will attempt to re-sign it.
		ThresholdSignatureInvalid { broadcast_attempt_id: BroadcastAttemptId },
		/// A signature accepted event on the target chain has been witnessed and the callback was
		/// executed.
		BroadcastCallbackExecuted { broadcast_id: BroadcastId, result: DispatchResult },
		/// The fee paid for broadcasting a transaction has been recorded.
		TransactionFeeDeficitRecorded {
			beneficiary: SignerIdFor<T, I>,
			amount: ChainAmountFor<T, I>,
		},
		/// The fee paid for broadcasting a transaction has been refused.
		TransactionFeeDeficitRefused { beneficiary: SignerIdFor<T, I> },
		/// A Call has been re-threshold-signed, and its signature data is inserted into storage.
		CallResigned { broadcast_id: BroadcastId },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided payload is invalid.
		InvalidPayload,
		/// The provided broadcast id is invalid.
		InvalidBroadcastId,
		/// The provided broadcast attempt id is invalid.
		InvalidBroadcastAttemptId,
		/// The transaction signer is not signer who was nominated.
		InvalidSigner,
		/// A threshold signature was expected but not available.
		ThresholdSignatureUnavailable,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// The `on_initialize` hook for this pallet handles scheduled expiries.
		///
		/// /// ## Events
		///
		/// - [BroadcastAttemptTimeout](Event::BroadcastAttemptTimeout)
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			// NB: We don't want broadcasts that timeout to ever expire. We will keep retrying
			// forever. It's possible that the reason for timeout could be something like a chain
			// halt on the external chain. If the signature is valid then we expect it to succeed
			// eventually. For outlying, unknown unknowns, these can be something governance can
			// handle if absolutely necessary (though it likely never will be).
			let expiries = Timeouts::<T, I>::take(block_number);
			if T::SafeMode::get().retry_enabled {
				for attempt_id in expiries.iter() {
					if PendingBroadcasts::<T, I>::get()
						.binary_search(&attempt_id.broadcast_id)
						.is_ok()
					{
						Self::deposit_event(Event::<T, I>::BroadcastAttemptTimeout {
							broadcast_attempt_id: *attempt_id,
						});
						if let Some(broadcast_attempt) = Self::take_awaiting_broadcast(*attempt_id)
						{
							Self::start_next_broadcast_attempt(broadcast_attempt);
						}
					}
				}
			} else {
				Timeouts::<T, I>::insert(
					block_number.saturating_add(T::SafeModeBlockMargin::get()),
					expiries.clone(),
				);
			}
			T::WeightInfo::on_initialize(expiries.len() as u32)
		}

		// We want to retry broadcasts when we have free block space.
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			if T::SafeMode::get().retry_enabled {
				let next_broadcast_weight = T::WeightInfo::start_next_broadcast_attempt();

				let num_retries_that_fit = remaining_weight
					.ref_time()
					.checked_div(next_broadcast_weight.ref_time())
					.expect("start_next_broadcast_attempt weight should not be 0")
					as usize;

				let retries = BroadcastRetryQueue::<T, I>::mutate(|retry_queue| {
					let id_limit = BroadcastBarriers::<T, I>::get()
						.front()
						.copied()
						.unwrap_or(BroadcastId::max_value());
					retry_queue
						.extract_if(|broadcast| {
							broadcast.broadcast_attempt_id.broadcast_id <= id_limit
						})
						.take(num_retries_that_fit)
						.collect::<Vec<_>>()
				});

				let retries_len = retries.len();

				for retry in retries {
					// Check if the broadcast is pending
					if PendingBroadcasts::<T, I>::get()
						.binary_search(&retry.broadcast_attempt_id.broadcast_id)
						.is_ok()
					{
						Self::start_next_broadcast_attempt(retry);
					}
				}
				next_broadcast_weight.saturating_mul(retries_len as u64) as Weight
			} else {
				Weight::zero()
			}
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Submitted by the nominated node when they cannot sign the transaction.
		/// This triggers a retry of the signing of the transaction
		///
		/// ## Events
		///
		/// - N/A
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Error::InvalidBroadcastAttemptId)
		/// - [InvalidSigner](Error::InvalidSigner)
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::transaction_signing_failure())]
		pub fn transaction_signing_failure(
			origin: OriginFor<T>,
			broadcast_attempt_id: BroadcastAttemptId,
		) -> DispatchResultWithPostInfo {
			let extrinsic_signer = T::AccountRoleRegistry::ensure_validator(origin.clone())?.into();

			let signing_attempt = AwaitingBroadcast::<T, I>::get(broadcast_attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			// Only the nominated signer can say they failed to sign
			ensure!(signing_attempt.nominee == extrinsic_signer, Error::<T, I>::InvalidSigner);

			FailedBroadcasters::<T, I>::append(
				signing_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id,
				&extrinsic_signer,
			);

			// Schedule a failed attempt for retry when the next block is authored.
			// We will abort the broadcast once all authorities have attempt to sign the
			// transaction
			if signing_attempt.broadcast_attempt.broadcast_attempt_id.attempt_count ==
				T::EpochInfo::current_authority_count()
					.checked_sub(1)
					.expect("We must have at least one authority")
			{
				let broadcast_id =
					signing_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id;

				// We want to keep the broadcast details, but we don't need the list of failed
				// broadcasters any more.
				FailedBroadcasters::<T, I>::remove(broadcast_id);

				// Call the failed callback and clean up the callback storage.
				if let Some(callback) = RequestFailureCallbacks::<T, I>::take(broadcast_id) {
					Self::deposit_event(Event::<T, I>::BroadcastCallbackExecuted {
						broadcast_id,
						result: callback
							.dispatch_bypass_filter(OriginTrait::root())
							.map(|_| ())
							.map_err(|e| {
								log::warn!(
								"Broadcast failure callback execution has failed for broadcast {}.",
								broadcast_id
							);
								e.error
							}),
					});
				}
				RequestSuccessCallbacks::<T, I>::remove(broadcast_id);

				Self::deposit_event(Event::<T, I>::BroadcastAborted {
					broadcast_id: signing_attempt
						.broadcast_attempt
						.broadcast_attempt_id
						.broadcast_id,
				});
				Self::remove_pending_broadcast(&broadcast_attempt_id.broadcast_id);
				AbortedBroadcasts::<T, I>::append(broadcast_attempt_id.broadcast_id);
			} else {
				Self::schedule_for_retry(&signing_attempt.broadcast_attempt);
			}

			Ok(().into())
		}

		/// A callback to be used when a threshold signature request completes. Retrieves the
		/// requested signature, uses the configured [TransactionBuilder] to build the transaction.
		/// Initiates the broadcast sequence if `should_broadcast` is set to true, otherwise insert
		/// the signature result into the `ThresholdSignatureData` storage.
		///
		/// ## Events
		///
		/// - [Event::CallResigned] If the call was re-signed.
		///
		///
		/// ## Errors
		///
		/// - [Error::ThresholdSignatureUnavailable]
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::on_signature_ready())]
		pub fn on_signature_ready(
			origin: OriginFor<T>,
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: PayloadFor<T, I>,
			api_call: Box<<T as Config<I>>::ApiCall>,
			broadcast_attempt_id: BroadcastAttemptId,
			initiated_at: ChainBlockNumberFor<T, I>,
			should_broadcast: bool,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureThresholdSigned::ensure_origin(origin)?;

			let signature = T::ThresholdSigner::signature_result(threshold_request_id)
				.ready_or_else(|r| {
					log::error!(
						"Signature not found for threshold request {:?}. Request status: {:?}",
						threshold_request_id,
						r
					);
					Error::<T, I>::ThresholdSignatureUnavailable
				})?
				.expect("signature can not be unavailable");

			let signed_api_call = api_call.signed(&signature);

			ThresholdSignatureData::<T, I>::insert(
				broadcast_attempt_id.broadcast_id,
				(signed_api_call.clone(), signature),
			);

			// If a signed call already exists, update the storage and do not broadcast.
			if should_broadcast {
				let transaction_out_id = signed_api_call.transaction_out_id();

				T::BroadcastReadyProvider::on_broadcast_ready(&signed_api_call);

				// The Engine uses this.
				TransactionOutIdToBroadcastId::<T, I>::insert(
					&transaction_out_id,
					(broadcast_attempt_id.broadcast_id, initiated_at),
				);

				let broadcast_attempt = BroadcastAttempt::<T, I> {
					broadcast_attempt_id,
					transaction_payload: T::TransactionBuilder::build_transaction(&signed_api_call),
					threshold_signature_payload,
					transaction_out_id,
				};

				if BroadcastBarriers::<T, I>::get().front().is_some_and(|broadcast_barrier_id| {
					broadcast_attempt_id.broadcast_id > *broadcast_barrier_id
				}) {
					Self::schedule_for_retry(&broadcast_attempt);
				} else {
					Self::start_broadcast_attempt(broadcast_attempt);
				}
			} else {
				Self::deposit_event(Event::<T, I>::CallResigned {
					broadcast_id: broadcast_attempt_id.broadcast_id,
				});
			}

			Ok(().into())
		}

		/// Nodes have witnessed that a signature was accepted on the target chain.
		///
		/// We add to the deficit to later be refunded, and clean up storage related to
		/// this broadcast, reporting any nodes who failed this particular broadcast before
		/// this success.
		///
		/// ## Events
		///
		/// - [BroadcastSuccess](Event::BroadcastSuccess)
		///
		/// ## Errors
		///
		/// - [InvalidPayload](Event::InvalidPayload)
		/// - [InvalidBroadcastAttemptId](Event::InvalidBroadcastAttemptId)
		#[pallet::weight(T::WeightInfo::transaction_succeeded())]
		#[pallet::call_index(2)]
		pub fn transaction_succeeded(
			origin: OriginFor<T>,
			tx_out_id: TransactionOutIdFor<T, I>,
			signer_id: SignerIdFor<T, I>,
			tx_fee: TransactionFeeFor<T, I>,
			tx_metadata: TransactionMetadataFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin.clone())?;

			let (broadcast_id, _initiated_at) =
				TransactionOutIdToBroadcastId::<T, I>::take(&tx_out_id)
					.ok_or(Error::<T, I>::InvalidPayload)?;

			Self::remove_pending_broadcast(&broadcast_id);

			if let Some(broadcast_barrier_id) = BroadcastBarriers::<T, I>::get().front() {
				if PendingBroadcasts::<T, I>::get()
					.first()
					.map_or(true, |id| *id > *broadcast_barrier_id)
				{
					BroadcastBarriers::<T, I>::mutate(|broadcast_barriers| {
						broadcast_barriers.pop_front();
					});
				}
			}

			if let Some(expected_tx_metadata) = TransactionMetadata::<T, I>::take(broadcast_id) {
				if tx_metadata.verify_metadata(&expected_tx_metadata) {
					if let Some(to_refund) = AwaitingBroadcast::<T, I>::get(BroadcastAttemptId {
						broadcast_id,
						attempt_count: BroadcastAttemptCount::<T, I>::get(broadcast_id),
					})
					.map(|signing_attempt| {
						signing_attempt
							.broadcast_attempt
							.transaction_payload
							.return_fee_refund(tx_fee)
					}) {
						TransactionFeeDeficit::<T, I>::mutate(signer_id.clone(), |fee_deficit| {
							*fee_deficit = fee_deficit.saturating_add(to_refund);
						});

						Self::deposit_event(Event::<T, I>::TransactionFeeDeficitRecorded {
							beneficiary: signer_id,
							amount: to_refund,
						});
					} else {
						log::warn!(
							"Unable to attribute transaction fee refundfor broadcast {}.",
							broadcast_id
						);
					}
				} else {
					Self::deposit_event(Event::<T, I>::TransactionFeeDeficitRefused {
						beneficiary: signer_id,
					});
					log::warn!(
						"Transaction metadata verification failed for broadcast {}. Deficit will not be recorded.",
						broadcast_id
					);
				}
			} else {
				log::error!(
					"Transaction metadata not found for broadcast {}. Deficit will be ignored.",
					broadcast_id
				);
			}

			if let Some(callback) = RequestSuccessCallbacks::<T, I>::get(broadcast_id) {
				Self::deposit_event(Event::<T, I>::BroadcastCallbackExecuted {
					broadcast_id,
					result: callback.dispatch_bypass_filter(origin.clone()).map(|_| ()).map_err(
						|e| {
							log::warn!(
								"Callback execution has failed for broadcast {}.",
								broadcast_id
							);
							e.error
						},
					),
				});
			}

			// Report the people who failed to broadcast this tx during its whole lifetime.
			if let Some(failed_signers) = FailedBroadcasters::<T, I>::take(broadcast_id) {
				T::OffenceReporter::report_many(
					PalletOffence::FailedToBroadcastTransaction,
					&failed_signers,
				);
			}

			Self::clean_up_broadcast_storage(broadcast_id);

			Self::deposit_event(Event::<T, I>::BroadcastSuccess {
				broadcast_id,
				transaction_out_id: tx_out_id,
			});
			Ok(().into())
		}

		// TODO: Remove this before mainnet (or use a feature flag?)
		#[pallet::weight(Weight::zero())]
		#[pallet::call_index(3)]
		pub fn stress_test(origin: OriginFor<T>, how_many: u32) -> DispatchResult {
			ensure_root(origin)?;

			let payload = PayloadFor::<T, I>::decode(&mut &[0xcf; 32][..])
				.map_err(|_| Error::<T, I>::InvalidPayload)?;
			for _ in 0..how_many {
				T::ThresholdSigner::request_signature(payload.clone());
			}

			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	pub fn clean_up_broadcast_storage(broadcast_id: BroadcastId) {
		for attempt_count in 0..=(BroadcastAttemptCount::<T, I>::take(broadcast_id)) {
			AwaitingBroadcast::<T, I>::remove(BroadcastAttemptId { broadcast_id, attempt_count });
		}

		TransactionMetadata::<T, I>::remove(broadcast_id);
		RequestSuccessCallbacks::<T, I>::remove(broadcast_id);
		RequestFailureCallbacks::<T, I>::remove(broadcast_id);
		if let Some((api_call, _)) = ThresholdSignatureData::<T, I>::take(broadcast_id) {
			TransactionOutIdToBroadcastId::<T, I>::remove(api_call.transaction_out_id());
		}
	}

	pub fn take_awaiting_broadcast(
		broadcast_attempt_id: BroadcastAttemptId,
	) -> Option<BroadcastAttempt<T, I>> {
		if let Some(signing_attempt) = AwaitingBroadcast::<T, I>::take(broadcast_attempt_id) {
			assert_eq!(
				signing_attempt.broadcast_attempt.broadcast_attempt_id,
				broadcast_attempt_id,
				"The broadcast attempt id of the signing attempt should match that of the broadcast attempt id of its key"
			);
			Some(signing_attempt.broadcast_attempt)
		} else {
			None
		}
	}

	pub fn remove_pending_broadcast(broadcast_id: &BroadcastId) {
		PendingBroadcasts::<T, I>::mutate(|pending_broadcasts| {
			debug_assert!(pending_broadcasts.iter().is_sorted());
			if let Ok(id) = pending_broadcasts.binary_search(broadcast_id) {
				pending_broadcasts.remove(id);
			} else {
				cf_runtime_utilities::log_or_panic!(
					"The broadcast_id should exist in the pending broadcasts list since we added it to the list when the broadcast was initated"
				);
			}
		});
	}

	/// Request a threshold signature, providing [Call::on_signature_ready] as the callback.
	pub fn threshold_sign_and_broadcast(
		api_call: <T as Config<I>>::ApiCall,
		maybe_success_callback: Option<<T as Config<I>>::BroadcastCallable>,
		maybe_failed_callback_generator: impl FnOnce(
			BroadcastId,
		) -> Option<<T as Config<I>>::BroadcastCallable>,
	) -> BroadcastId {
		let broadcast_id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		PendingBroadcasts::<T, I>::append(broadcast_id);

		if let Some(callback) = maybe_success_callback {
			RequestSuccessCallbacks::<T, I>::insert(broadcast_id, callback);
		}
		if let Some(callback) = maybe_failed_callback_generator(broadcast_id) {
			RequestFailureCallbacks::<T, I>::insert(broadcast_id, callback);
		}

		let _threshold_signature_id = Self::threshold_sign(api_call, broadcast_id, true);

		broadcast_id
	}

	/// Signs a API call, use `Call::on_signature_ready` as the callback, and returns the signature
	/// request ID.
	fn threshold_sign(
		api_call: <T as Config<I>>::ApiCall,
		broadcast_id: BroadcastId,
		should_broadcast: bool,
	) -> ThresholdSignatureRequestId {
		// We must set this here because after the threshold signature is requested, it's
		// possible that an authority submits the transaction themselves, not going through the
		// standard path. This protects against that, to ensure we always set the earliest possible
		// block number we could have broadcast at, so that we can ensure we witness it.
		let initiated_at = T::ChainTracking::get_block_height();

		let threshold_signature_payload = api_call.threshold_signature_payload();
		T::ThresholdSigner::request_signature_with_callback(
			threshold_signature_payload.clone(),
			|threshold_request_id| {
				Call::on_signature_ready {
					threshold_request_id,
					threshold_signature_payload,
					api_call: Box::new(api_call),
					broadcast_attempt_id: BroadcastAttemptId { broadcast_id, attempt_count: 0 },
					initiated_at,
					should_broadcast,
				}
				.into()
			},
		)
	}

	fn start_next_broadcast_attempt(broadcast_attempt: BroadcastAttempt<T, I>) {
		let broadcast_id = broadcast_attempt.broadcast_attempt_id.broadcast_id;

		if let Some((api_call, signature)) = ThresholdSignatureData::<T, I>::get(broadcast_id) {
			let EpochKey { key, .. } = T::KeyProvider::active_epoch_key()
				.expect("Epoch key must exist if we made a broadcast.");

			let next_broadcast_attempt_id =
				broadcast_attempt.broadcast_attempt_id.into_next::<T, I>();

			if T::TransactionBuilder::is_valid_for_rebroadcast(
				&api_call,
				&broadcast_attempt.threshold_signature_payload,
				&key,
				&signature,
			) {
				Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
					broadcast_attempt_id: next_broadcast_attempt_id,
					..broadcast_attempt
				});
			}
			// If the signature verification fails, we want
			// to retry from the threshold signing stage.
			else {
				// We update the initiated_at here since as the tx is resigned and broadcast, it is
				// not possible for it to be successfully broadcasted before this point.
				// This `initiated_at` block will be associated with the new transaction_out_id
				// so should not interfere with witnessing the previous one.
				let initiated_at = T::ChainTracking::get_block_height();

				Self::deposit_event(Event::<T, I>::ThresholdSignatureInvalid {
					broadcast_attempt_id: broadcast_attempt.broadcast_attempt_id,
				});

				let threshold_signature_payload = api_call.threshold_signature_payload();
				T::ThresholdSigner::request_signature_with_callback(
					threshold_signature_payload.clone(),
					|threshold_request_id| {
						Call::on_signature_ready {
							threshold_request_id,
							threshold_signature_payload,
							api_call: Box::new(api_call),
							broadcast_attempt_id: next_broadcast_attempt_id,
							initiated_at,
							should_broadcast: true,
						}
						.into()
					},
				);

				log::info!(
					"Signature is invalid -> rescheduled threshold signature for broadcast id {}.",
					broadcast_id
				);
			}
		} else {
			log::error!("No threshold signature data is available.");
		};
	}

	fn start_broadcast_attempt(mut broadcast_attempt: BroadcastAttempt<T, I>) {
		T::TransactionBuilder::refresh_unsigned_data(&mut broadcast_attempt.transaction_payload);
		TransactionMetadata::<T, I>::insert(
			broadcast_attempt.broadcast_attempt_id.broadcast_id,
			<<T::TargetChain as Chain>::TransactionMetadata>::extract_metadata(
				&broadcast_attempt.transaction_payload,
			),
		);

		let seed =
			(broadcast_attempt.broadcast_attempt_id, broadcast_attempt.transaction_payload.clone())
				.encode();
		if let Some(nominated_signer) = T::BroadcastSignerNomination::nominate_broadcaster(
			seed,
			&FailedBroadcasters::<T, I>::get(broadcast_attempt.broadcast_attempt_id.broadcast_id)
				.unwrap_or_default(),
		) {
			// write, or overwrite the old entry if it exists (on a retry)
			AwaitingBroadcast::<T, I>::insert(
				broadcast_attempt.broadcast_attempt_id,
				TransactionSigningAttempt {
					broadcast_attempt: BroadcastAttempt::<T, I> {
						transaction_payload: broadcast_attempt.transaction_payload.clone(),
						transaction_out_id: broadcast_attempt.transaction_out_id.clone(),
						..broadcast_attempt
					},
					nominee: nominated_signer.clone(),
				},
			);

			Timeouts::<T, I>::append(
				frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get(),
				broadcast_attempt.broadcast_attempt_id,
			);

			Self::deposit_event(Event::<T, I>::TransactionBroadcastRequest {
				broadcast_attempt_id: broadcast_attempt.broadcast_attempt_id,
				nominee: nominated_signer,
				transaction_payload: broadcast_attempt.transaction_payload,
				transaction_out_id: broadcast_attempt.transaction_out_id,
			});
		} else {
			log::warn!(
				"Failed to select a signer for broadcast {:?}. Scheduling Retry",
				broadcast_attempt.broadcast_attempt_id
			);
			Self::schedule_for_retry(&broadcast_attempt);
		}
	}

	fn schedule_for_retry(broadcast_attempt: &BroadcastAttempt<T, I>) {
		BroadcastRetryQueue::<T, I>::append(broadcast_attempt);
		Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled {
			broadcast_attempt_id: broadcast_attempt.broadcast_attempt_id,
		});
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	type Callback = <T as Config<I>>::BroadcastCallable;

	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) -> BroadcastId {
		Self::threshold_sign_and_broadcast(api_call, None, |_| None)
	}

	fn threshold_sign_and_broadcast_with_callback(
		api_call: Self::ApiCall,
		success_callback: Option<Self::Callback>,
		failed_callback_generator: impl FnOnce(BroadcastId) -> Option<Self::Callback>,
	) -> BroadcastId {
		Self::threshold_sign_and_broadcast(api_call, success_callback, failed_callback_generator)
	}

	fn threshold_resign(broadcast_id: BroadcastId) -> Option<ThresholdSignatureRequestId> {
		ThresholdSignatureData::<T, I>::get(broadcast_id)
			.map(|(api_call, _signature)| Self::threshold_sign(api_call, broadcast_id, false))
	}

	/// Clean up storage data related to a broadcast ID.
	fn clean_up_broadcast_storage(broadcast_id: BroadcastId) {
		Self::clean_up_broadcast_storage(broadcast_id);
	}

	fn threshold_sign_and_broadcast_rotation_tx(api_call: Self::ApiCall) -> BroadcastId {
		let broadcast_id = <Self as Broadcaster<_>>::threshold_sign_and_broadcast(api_call);

		BroadcastBarriers::<T, I>::mutate(|current_barriers| {
			current_barriers.append(
				&mut <<<T as pallet::Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::maybe_broadcast_barriers_on_rotation(broadcast_id)
					.extract_if(|barrier| {
						PendingBroadcasts::<T, I>::get().first().map_or(false, |id| *id <= *barrier)
					})
					.collect::<VecDeque<BroadcastId>>(),
			);
		});
		broadcast_id
	}
}
