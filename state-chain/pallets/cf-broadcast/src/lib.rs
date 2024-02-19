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

use cf_chains::{
	ApiCall, Chain, ChainCrypto, FeeRefundCalculator, RetryPolicy, TransactionBuilder,
	TransactionMetadata as _,
};
use cf_traits::{
	offence_reporting::OffenceReporter, BroadcastNomination, Broadcaster, CfeBroadcastRequest,
	Chainflip, EpochInfo, GetBlockHeight, SafeMode, ThresholdSigner,
};
use cfe_events::TxBroadcastRequest;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	pallet_prelude::{ensure, DispatchResult, RuntimeDebug},
	sp_runtime::traits::{One, Saturating},
	traits::{Defensive, Get, OriginTrait, StorageVersion, UnfilteredDispatchable},
	Twox64Concat,
};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
pub use pallet::*;
use scale_info::TypeInfo;
use sp_std::{collections::btree_set::BTreeSet, marker, marker::PhantomData, prelude::*, vec::Vec};
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

/// The number of broadcast attempts that were made before this one.
pub type AttemptCount = u32;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedToBroadcastTransaction,
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::benchmarking_value::BenchmarkValue;
	use cf_traits::{AccountRoleRegistry, BroadcastNomination, OnBroadcastReady};
	use frame_support::{pallet_prelude::*, traits::EnsureOrigin};
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

	/// All data contained in a Broadcast
	#[derive(RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, CloneNoBound)]
	#[scale_info(skip_type_params(T, I))]
	pub struct BroadcastData<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub transaction_payload: TransactionFor<T, I>,
		pub threshold_signature_payload: PayloadFor<T, I>,
		pub transaction_out_id: TransactionOutIdFor<T, I>,
		pub nominee: Option<T::ValidatorId>,
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

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;

		/// The save mode block margin
		type SafeModeBlockMargin: Get<BlockNumberFor<Self>>;

		/// The policy on which decide when we slow down the retry of a broadcast.
		type RetryPolicy: RetryPolicy<
			BlockNumber = BlockNumberFor<Self>,
			AttemptCount = AttemptCount,
		>;

		type CfeBroadcastRequest: CfeBroadcastRequest<Self, Self::TargetChain>;

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

	/// Contains a Set of the authorities that have failed to sign a particular broadcast.
	#[pallet::storage]
	pub type FailedBroadcasters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, BTreeSet<T::ValidatorId>, ValueQuery>;

	/// Live transaction broadcast requests.
	#[pallet::storage]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, BroadcastData<T, I>, OptionQuery>;

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

	/// The list of failed broadcasts that will be retried in some future block.
	#[pallet::storage]
	pub type DelayedBroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, BTreeSet<BroadcastId>, ValueQuery>;

	/// A mapping from block number to a list of broadcasts that expire at that
	/// block number.
	#[pallet::storage]
	pub type Timeouts<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		BTreeSet<(BroadcastId, T::ValidatorId)>,
		ValueQuery,
	>;

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
	pub type BroadcastBarriers<T, I = ()> = StorageValue<_, BTreeSet<BroadcastId>, ValueQuery>;

	/// List of broadcasts that are initiated but not witnessed on the external chain.
	#[pallet::storage]
	#[pallet::getter(fn pending_broadcasts)]
	pub type PendingBroadcasts<T, I = ()> = StorageValue<_, BTreeSet<BroadcastId>, ValueQuery>;

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
			broadcast_id: BroadcastId,
			nominee: T::ValidatorId,
			transaction_payload: TransactionFor<T, I>,
			transaction_out_id: TransactionOutIdFor<T, I>,
		},
		/// A failed broadcast has been scheduled for retry.
		BroadcastRetryScheduled { broadcast_id: BroadcastId, retry_block: BlockNumberFor<T> },
		/// A broadcast has timed out.
		BroadcastTimeout { broadcast_id: BroadcastId },
		/// A broadcast has been aborted after all authorities have failed to broadcast it.
		BroadcastAborted { broadcast_id: BroadcastId },
		/// A broadcast has successfully been completed.
		BroadcastSuccess {
			broadcast_id: BroadcastId,
			transaction_out_id: TransactionOutIdFor<T, I>,
		},
		/// A broadcast's threshold signature is invalid, we will attempt to re-sign it.
		ThresholdSignatureInvalid { broadcast_id: BroadcastId },
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
		/// A threshold signature was expected but not available.
		ThresholdSignatureUnavailable,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Process any broadcasts that expired are treated as failed.
		/// Re-try any broadcasts in the Delayed Retry queue.
		/// If safe mode prevents retrying, the broadcasts are added to future blocks.
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			// We treat a time out here as a Broadcast Failure. This is handled the same way - the
			// current broadcaster is reported as Failed to broadcast, and a new broadcaster is
			// nominated. If there are no more broadcaster available, then the broadcast is aborted.
			let mut expiries = Timeouts::<T, I>::take(block_number);
			let pending_broadcasts = PendingBroadcasts::<T, I>::get();
			let mut delayed_retries = DelayedBroadcastRetryQueue::<T, I>::take(block_number);
			let expiry_count = expiries.len();
			let retries_count = delayed_retries.len();
			if T::SafeMode::get().retry_enabled {
				for (broadcast_id, nominee) in expiries {
					if pending_broadcasts.contains(&broadcast_id) {
						Self::deposit_event(Event::<T, I>::BroadcastTimeout { broadcast_id });
						if let Err(e) = Self::handle_broadcast_failure(broadcast_id, nominee) {
							log::warn!("Error when handling broadcast failure: Broadcast ID:{}, Error: {:?}", broadcast_id, e);
						}
					}
				}

				// Retry broadcast (allowed by broadcast barrier)
				let next_block = block_number.saturating_add(One::one());
				let id_limit = BroadcastBarriers::<T, I>::get()
					.first()
					.copied()
					.unwrap_or(BroadcastId::max_value());
				delayed_retries.retain(|broadcast_id| {
					if *broadcast_id <= id_limit {
						// If retry is allowed by the barrier - start the retry.
						Self::start_next_broadcast_attempt(*broadcast_id);
						false
					} else {
						true
					}
				});
				if !delayed_retries.is_empty() {
					DelayedBroadcastRetryQueue::<T, I>::mutate(next_block, |current| {
						current.append(&mut delayed_retries)
					});
				}
			} else {
				Timeouts::<T, I>::mutate(
					block_number.saturating_add(T::SafeModeBlockMargin::get()),
					|current| current.append(&mut expiries),
				);
				DelayedBroadcastRetryQueue::<T, I>::mutate(
					block_number.saturating_add(T::SafeModeBlockMargin::get()),
					|current| current.append(&mut delayed_retries),
				);
			}
			T::WeightInfo::on_initialize(expiry_count as u32, retries_count as u32)
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// DEPRECATED. This call is no longer used.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::transaction_failed())]
		pub fn transaction_signing_failure(
			_origin: OriginFor<T>,
			_broadcast_attempt_id: (BroadcastId, AttemptCount),
		) -> DispatchResultWithPostInfo {
			Err(DispatchError::Other("Deprecated").into())
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
			broadcast_id: BroadcastId,
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
				broadcast_id,
				(signed_api_call.clone(), signature),
			);

			// If a signed call already exists, update the storage and do not broadcast.
			if should_broadcast {
				let transaction_out_id = signed_api_call.transaction_out_id();

				T::BroadcastReadyProvider::on_broadcast_ready(&signed_api_call);

				// The Engine uses this.
				TransactionOutIdToBroadcastId::<T, I>::insert(
					&transaction_out_id,
					(broadcast_id, initiated_at),
				);

				let broadcast_data = BroadcastData::<T, I> {
					broadcast_id,
					transaction_payload: T::TransactionBuilder::build_transaction(&signed_api_call),
					threshold_signature_payload,
					transaction_out_id,
					nominee: None,
				};
				AwaitingBroadcast::<T, I>::insert(broadcast_id, broadcast_data.clone());

				if BroadcastBarriers::<T, I>::get()
					.first()
					.is_some_and(|broadcast_barrier_id| broadcast_id > *broadcast_barrier_id)
				{
					Self::schedule_for_retry(broadcast_id);
				} else {
					Self::start_broadcast_attempt(broadcast_data);
				}
			} else {
				Self::deposit_event(Event::<T, I>::CallResigned { broadcast_id });
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
		/// - [InvalidBroadcastId](Event::InvalidBroadcastId)
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

			if let Some(expected_tx_metadata) = TransactionMetadata::<T, I>::take(broadcast_id) {
				if tx_metadata.verify_metadata(&expected_tx_metadata) {
					if let Some(broadcast_data) = AwaitingBroadcast::<T, I>::get(broadcast_id) {
						let to_refund =
							broadcast_data.transaction_payload.return_fee_refund(tx_fee);

						TransactionFeeDeficit::<T, I>::mutate(signer_id.clone(), |fee_deficit| {
							*fee_deficit = fee_deficit.saturating_add(to_refund);
						});

						Self::deposit_event(Event::<T, I>::TransactionFeeDeficitRecorded {
							beneficiary: signer_id,
							amount: to_refund,
						});
					} else {
						log::warn!(
							"Unable to attribute transaction fee refund for broadcast {}.",
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
			let failed_broadcasters = FailedBroadcasters::<T, I>::take(broadcast_id);
			if !failed_broadcasters.is_empty() {
				T::OffenceReporter::report_many(
					PalletOffence::FailedToBroadcastTransaction,
					failed_broadcasters,
				);
			}

			Self::clean_up_broadcast_storage(broadcast_id);

			Self::deposit_event(Event::<T, I>::BroadcastSuccess {
				broadcast_id,
				transaction_out_id: tx_out_id,
			});
			Ok(().into())
		}

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

		/// Submitted by the nominated node to signal that they were unable to broadcast the
		/// transaction.
		///
		/// ## Events
		///
		/// - N/A
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastId](Error::InvalidBroadcastId)
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::transaction_failed())]
		pub fn transaction_failed(
			origin: OriginFor<T>,
			broadcast_id: BroadcastId,
		) -> DispatchResultWithPostInfo {
			let reporter = T::AccountRoleRegistry::ensure_validator(origin.clone())?;

			Self::handle_broadcast_failure(broadcast_id, reporter.into())?;
			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	pub fn clean_up_broadcast_storage(broadcast_id: BroadcastId) {
		AwaitingBroadcast::<T, I>::remove(broadcast_id);
		TransactionMetadata::<T, I>::remove(broadcast_id);
		RequestSuccessCallbacks::<T, I>::remove(broadcast_id);
		RequestFailureCallbacks::<T, I>::remove(broadcast_id);
		if let Some((api_call, _)) = ThresholdSignatureData::<T, I>::take(broadcast_id) {
			TransactionOutIdToBroadcastId::<T, I>::remove(api_call.transaction_out_id());
		}
	}

	pub fn remove_pending_broadcast(broadcast_id: &BroadcastId) {
		PendingBroadcasts::<T, I>::mutate(|pending_broadcasts| {
			if !pending_broadcasts.remove(broadcast_id) {
				log::warn!("Expected broadcast with id {} to still be pending.", broadcast_id);
			}
			if let Some(broadcast_barrier_id) = BroadcastBarriers::<T, I>::get().first() {
				if pending_broadcasts.first().map_or(true, |id| *id > *broadcast_barrier_id) {
					BroadcastBarriers::<T, I>::mutate(|broadcast_barriers| {
						broadcast_barriers.pop_first();
					});
				}
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
		let broadcast_id = Self::next_broadcast_id();

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
					broadcast_id,
					initiated_at,
					should_broadcast,
				}
				.into()
			},
		)
	}

	fn start_next_broadcast_attempt(broadcast_id: BroadcastId) {
		if !PendingBroadcasts::<T, I>::get().contains(&broadcast_id) {
			log::warn!(
				"Broadcast already succeeded or aborted, retry is ignored. broadcast_id: {}",
				broadcast_id
			);
			return
		}

		if let Some(broadcast_data) = AwaitingBroadcast::<T, I>::get(broadcast_id) {
			// If the broadcast is not pending, we should not retry.
			if let Some((api_call, _signature)) = ThresholdSignatureData::<T, I>::get(broadcast_id)
			{
				if T::TransactionBuilder::requires_signature_refresh(
					&api_call,
					&broadcast_data.threshold_signature_payload,
				) {
					// We update the initiated_at here since as the tx is resigned and broadcast, it
					// is not possible for it to be successfully broadcasted before this point.
					// This `initiated_at` block will be associated with the new transaction_out_id
					// so should not interfere with witnessing the previous one.
					let initiated_at = T::ChainTracking::get_block_height();

					Self::deposit_event(Event::<T, I>::ThresholdSignatureInvalid { broadcast_id });

					let threshold_signature_payload = api_call.threshold_signature_payload();
					T::ThresholdSigner::request_signature_with_callback(
						threshold_signature_payload.clone(),
						|threshold_request_id| {
							Call::on_signature_ready {
								threshold_request_id,
								threshold_signature_payload,
								api_call: Box::new(api_call),
								broadcast_id,
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
				} else {
					Self::start_broadcast_attempt(broadcast_data);
				}
			} else {
				log::error!("No threshold signature data are available for broadcast: {:?}. Retry is aborted.", broadcast_id);
			};
		} else {
			log::error!(
				"Broadcast data not found for broadcast: {:?}. Retry is aborted.",
				broadcast_id
			);
		}
	}

	/// Start a broadcast by try to select a nominee.
	fn start_broadcast_attempt(mut broadcast_data: BroadcastData<T, I>) {
		let broadcast_id = broadcast_data.broadcast_id;
		T::TransactionBuilder::refresh_unsigned_data(&mut broadcast_data.transaction_payload);
		TransactionMetadata::<T, I>::insert(
			broadcast_id,
			<<T::TargetChain as Chain>::TransactionMetadata>::extract_metadata(
				&broadcast_data.transaction_payload,
			),
		);

		// Pass in the current block number as part of the seed to achieve pseudo-randomness.
		if let Some(nominated_signer) = T::BroadcastSignerNomination::nominate_broadcaster(
			(broadcast_id, frame_system::Pallet::<T>::block_number()),
			FailedBroadcasters::<T, I>::get(broadcast_id),
		) {
			// Overwrite the old entry with updated broadcast data.
			broadcast_data.nominee = Some(nominated_signer.clone());
			AwaitingBroadcast::<T, I>::insert(broadcast_id, broadcast_data.clone());

			Timeouts::<T, I>::append(
				frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get(),
				(broadcast_id, nominated_signer.clone()),
			);

			T::CfeBroadcastRequest::tx_broadcast_request(TxBroadcastRequest {
				broadcast_id,
				nominee: nominated_signer.clone(),
				payload: broadcast_data.transaction_payload.clone(),
			});

			// TODO: consider removing this
			Self::deposit_event(Event::<T, I>::TransactionBroadcastRequest {
				broadcast_id,
				nominee: nominated_signer,
				transaction_payload: broadcast_data.transaction_payload,
				transaction_out_id: broadcast_data.transaction_out_id,
			});
		} else {
			log::debug!(
				"Failed to nominate a broadcaster, but not all validators have reported failure. Broadcast is scheduled for retry. Broadcast Id: {}",
				broadcast_id
			);
			// Schedule for retry later, when more broadcasters become available.
			Self::schedule_for_retry(broadcast_id);
		}
	}

	fn schedule_for_retry(broadcast_id: BroadcastId) {
		// If no delay, retry in the next block.
		let retry_block = frame_system::Pallet::<T>::block_number().saturating_add(
			T::RetryPolicy::next_attempt_delay(Self::attempt_count(broadcast_id))
				.unwrap_or(One::one()),
		);

		DelayedBroadcastRetryQueue::<T, I>::append(retry_block, broadcast_id);

		Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled { broadcast_id, retry_block });
	}

	// Advance the broadcast ID in storage by 1 and return the result.
	fn next_broadcast_id() -> BroadcastId {
		BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		})
	}

	/// Handles a broadcast failure. The reporter is added to a list of FailedBroadcasters to be
	/// slashed later. If no reporter is given, the Nominated broadcast is used instead.
	/// The broadcast will then be retried.
	fn handle_broadcast_failure(
		broadcast_id: BroadcastId,
		failed_broadcaster: T::ValidatorId,
	) -> DispatchResult {
		ensure!(
			PendingBroadcasts::<T, I>::get().contains(&broadcast_id),
			Error::<T, I>::InvalidBroadcastId
		);

		if let Ok(attempt_count) =
			FailedBroadcasters::<T, I>::try_mutate(broadcast_id, |failed_broadcasters| {
				if failed_broadcasters.insert(failed_broadcaster.clone()) {
					Ok(failed_broadcasters.len())
				} else {
					Err(())
				}
			}) {
			// Abort the broadcast if all validators reported failure, Retry otherwise.
			if attempt_count >= T::EpochInfo::current_authority_count() as usize {
				Self::abort_broadcast(broadcast_id);
			} else {
				Self::schedule_for_retry(broadcast_id);
			}
		} else {
			// Do nothing since this failure has already been reported.
			log::warn!(
				"Broadcast failure by {:?} already recorded for Broadcast ID: {}",
				failed_broadcaster,
				broadcast_id
			);
		}

		Ok(())
	}

	/// Called when all validators have failed to broadcast this call. We abort to prevent infinite
	/// retries. The failed callback is dispatched (if any), and data is kept in storage for
	/// potential future governance functions.
	fn abort_broadcast(broadcast_id: BroadcastId) {
		log::warn!(
			"All authorities failed to broadcast, broadcast is aborted. Broadcast_id {:?}.",
			broadcast_id
		);

		// We want to keep the broadcast details, but we don't need the list of failed
		// broadcasters any more.
		FailedBroadcasters::<T, I>::remove(broadcast_id);

		// Call the failed callback and clean up the callback storage.
		if let Some(callback) = RequestFailureCallbacks::<T, I>::take(broadcast_id) {
			Self::deposit_event(Event::<T, I>::BroadcastCallbackExecuted {
				broadcast_id,
				result: callback.dispatch_bypass_filter(OriginTrait::root()).map(|_| ()).map_err(
					|e| {
						log::error!(
							"Broadcast failure callback execution has failed for broadcast {}.",
							broadcast_id
						);
						e.error
					},
				),
			});
		}
		RequestSuccessCallbacks::<T, I>::remove(broadcast_id);

		Self::deposit_event(Event::<T, I>::BroadcastAborted { broadcast_id });
		Self::remove_pending_broadcast(&broadcast_id);
		AbortedBroadcasts::<T, I>::append(broadcast_id);
	}

	pub fn attempt_count(broadcast_id: BroadcastId) -> AttemptCount {
		// NOTE: decode_non_dedup_len is correct here only as long as we *don't* use `append` to
		// insert items.
		FailedBroadcasters::<T, I>::decode_non_dedup_len(broadcast_id).unwrap_or_default() as u32
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

	fn threshold_sign(api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId) {
		let broadcast_id = Self::next_broadcast_id();
		(broadcast_id, Self::threshold_sign(api_call, broadcast_id, false))
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

		if let Some(earliest_pending_broadcast_id) = PendingBroadcasts::<T, I>::get()
			.first()
			.defensive_proof("Broadcast ID was just inserted, so at least this one must exist.")
		{
			for barrier in <<<T as pallet::Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::maybe_broadcast_barriers_on_rotation(broadcast_id) {
					if barrier >= *earliest_pending_broadcast_id {
						BroadcastBarriers::<T, I>::append(barrier);
					}
				}
		}
		broadcast_id
	}
}
