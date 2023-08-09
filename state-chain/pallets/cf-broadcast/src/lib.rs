#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;
mod mock;
mod tests;

pub mod weights;
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
pub use weights::WeightInfo;

use cf_chains::{ApiCall, Chain, ChainAbi, ChainCrypto, FeeRefundCalculator, TransactionBuilder};
use cf_traits::{
	offence_reporting::OffenceReporter, Broadcaster, Chainflip, EpochInfo, EpochKey,
	OnBroadcastReady, SingleSignerNomination, ThresholdSigner,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	pallet_prelude::DispatchResult,
	sp_runtime::traits::Saturating,
	traits::{Get, UnfilteredDispatchable},
	Twox64Concat,
};

use cf_traits::KeyProvider;

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
	/// Increment the attempt count for a particular BroadcastAttemptId
	pub fn next_attempt(&self) -> Self {
		Self { attempt_count: self.attempt_count.wrapping_add(1), ..*self }
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

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::benchmarking_value::BenchmarkValue;
	use cf_traits::{AccountRoleRegistry, KeyProvider, OnBroadcastReady, SingleSignerNomination};
	use frame_support::{ensure, pallet_prelude::*, traits::EnsureOrigin};
	use frame_system::pallet_prelude::*;

	/// Type alias for the instance's configured Transaction.
	pub type TransactionFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::Transaction;

	/// Type alias for the instance's configured SignerId.
	pub type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	/// Type alias for the payload hash
	pub type ThresholdSignatureFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

	pub type TransactionOutIdFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainCrypto>::TransactionOutId;

	/// Type alias for the instance's configured Payload.
	pub type PayloadFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::Payload;

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

	// TODO: Rename
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
		type TargetChain: ChainAbi;

		/// The api calls supported by this broadcaster.
		type ApiCall: ApiCall<Self::TargetChain> + BenchmarkValue;

		/// Builds the transaction according to the chain's environment settings.
		type TransactionBuilder: TransactionBuilder<Self::TargetChain, Self::ApiCall>;

		/// A threshold signer that can sign calls for this chain, and dispatch callbacks into this
		/// pallet.
		type ThresholdSigner: ThresholdSigner<
			Self::TargetChain,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Signer nomination.
		type BroadcastSignerNomination: SingleSignerNomination<SignerId = Self::ValidatorId>;

		/// For reporting bad actors.
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ensure that only threshold signature consensus can trigger a broadcast.
		type EnsureThresholdSigned: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;

		type BroadcastReadyProvider: OnBroadcastReady<Self::TargetChain, ApiCall = Self::ApiCall>;

		/// The timeout duration for the broadcast, measured in number of blocks.
		#[pallet::constant]
		type BroadcastTimeout: Get<BlockNumberFor<Self>>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<Self::TargetChain>;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Copy, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T, I))]
	pub struct Origin<T: Config<I>, I: 'static = ()>(pub(super) PhantomData<(T, I)>);

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter for incrementing the broadcast id.
	#[pallet::storage]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	/// Callbacks to be dispatched when the SignatureAccepted event has been witnessed.
	#[pallet::storage]
	#[pallet::getter(fn request_callback)]
	pub type RequestCallbacks<T: Config<I>, I: 'static = ()> =
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
	#[pallet::storage]
	pub type TransactionOutIdToBroadcastId<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TransactionOutIdFor<T, I>, BroadcastId, OptionQuery>;

	/// The list of failed broadcasts pending retry.
	#[pallet::storage]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<BroadcastAttempt<T, I>>, ValueQuery>;

	/// A mapping from block number to a list of signing or broadcast attempts that expire at that
	/// block number.
	#[pallet::storage]
	pub type Timeouts<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, T::BlockNumber, Vec<BroadcastAttemptId>, ValueQuery>;

	/// Stores all needed information to be able to re-request the signature
	#[pallet::storage]
	pub type ThresholdSignatureData<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastId,
		(ApiCallFor<T, I>, ThresholdSignatureFor<T, I>),
		OptionQuery,
	>;

	/// Tracks how much a signer id is owed for paying transaction fees.
	#[pallet::storage]
	pub type TransactionFeeDeficit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, SignerIdFor<T, I>, ChainAmountFor<T, I>, ValueQuery>;

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
		ThresholdSignatureInvalid { broadcast_id: BroadcastId },
		/// A signature accepted event on the target chain has been witnessed and the callback was
		/// executed.
		BroadcastCallbackExecuted { broadcast_id: BroadcastId, result: DispatchResult },
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
			for attempt_id in expiries.iter() {
				if let Some(attempt) = Self::take_awaiting_broadcast(*attempt_id) {
					Self::deposit_event(Event::<T, I>::BroadcastAttemptTimeout {
						broadcast_attempt_id: *attempt_id,
					});
					Self::start_next_broadcast_attempt(attempt);
				}
			}

			T::WeightInfo::on_initialize(expiries.len() as u32)
		}

		// We want to retry broadcasts when we have free block space.
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let next_broadcast_weight = T::WeightInfo::start_next_broadcast_attempt();

			let num_retries_that_fit = remaining_weight
				.ref_time()
				.checked_div(next_broadcast_weight.ref_time())
				.expect("start_next_broadcast_attempt weight should not be 0")
				as usize;

			let mut retries = BroadcastRetryQueue::<T, I>::take();

			if retries.len() >= num_retries_that_fit {
				BroadcastRetryQueue::<T, I>::put(retries.split_off(num_retries_that_fit));
			}

			let retries_len = retries.len();

			for retry in retries {
				Self::start_next_broadcast_attempt(retry);
			}
			next_broadcast_weight.saturating_mul(retries_len as u64) as Weight
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
		#[pallet::weight(T::WeightInfo::transaction_signing_failure())]
		pub fn transaction_signing_failure(
			origin: OriginFor<T>,
			broadcast_attempt_id: BroadcastAttemptId,
		) -> DispatchResultWithPostInfo {
			let extrinsic_signer = T::AccountRoleRegistry::ensure_validator(origin)?.into();

			let signing_attempt = AwaitingBroadcast::<T, I>::get(broadcast_attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			// Only the nominated signer can say they failed to sign
			ensure!(signing_attempt.nominee == extrinsic_signer, Error::<T, I>::InvalidSigner);

			Self::take_awaiting_broadcast(broadcast_attempt_id);

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
				Self::deposit_event(Event::<T, I>::BroadcastAborted {
					broadcast_id: signing_attempt
						.broadcast_attempt
						.broadcast_attempt_id
						.broadcast_id,
				});
			} else {
				BroadcastRetryQueue::<T, I>::append(&signing_attempt.broadcast_attempt);
				Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled {
					broadcast_attempt_id: signing_attempt.broadcast_attempt.broadcast_attempt_id,
				});
			}

			Ok(().into())
		}

		/// A callback to be used when a threshold signature request completes. Retrieves the
		/// requested signature, uses the configured [TransactionBuilder] to build the transaction
		/// and then initiates the broadcast sequence.
		///
		/// ## Events
		///
		/// - See [Call::start_broadcast].
		///
		/// ## Errors
		///
		/// - [Error::ThresholdSignatureUnavailable]
		#[pallet::weight(T::WeightInfo::on_signature_ready())]
		pub fn on_signature_ready(
			origin: OriginFor<T>,
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: PayloadFor<T, I>,
			api_call: Box<<T as Config<I>>::ApiCall>,
			broadcast_id: BroadcastId,
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

			Self::start_broadcast(
				&signature,
				T::TransactionBuilder::build_transaction(&signed_api_call),
				signed_api_call,
				threshold_signature_payload,
				broadcast_id,
			);
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
		pub fn transaction_succeeded(
			origin: OriginFor<T>,
			tx_out_id: TransactionOutIdFor<T, I>,
			signer_id: SignerIdFor<T, I>,
			tx_fee: TransactionFeeFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin.clone())?;

			let broadcast_id = TransactionOutIdToBroadcastId::<T, I>::take(&tx_out_id)
				.ok_or(Error::<T, I>::InvalidPayload)?;

			let to_refund = AwaitingBroadcast::<T, I>::get(BroadcastAttemptId {
				broadcast_id,
				attempt_count: BroadcastAttemptCount::<T, I>::get(broadcast_id),
			})
			.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?
			.broadcast_attempt
			.transaction_payload
			.return_fee_refund(tx_fee);

			TransactionFeeDeficit::<T, I>::mutate(signer_id, |fee_deficit| {
				*fee_deficit = fee_deficit.saturating_add(to_refund);
			});

			if let Some(callback) = RequestCallbacks::<T, I>::get(broadcast_id) {
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

		#[pallet::weight(0)]
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
		let first_attempt = AttemptCount::default();

		if let Some(transaction_signing_attempt) =
			AwaitingBroadcast::<T, I>::get(BroadcastAttemptId {
				broadcast_id,
				attempt_count: first_attempt,
			}) {
			TransactionOutIdToBroadcastId::<T, I>::remove(
				transaction_signing_attempt.broadcast_attempt.transaction_out_id,
			);
		};

		for attempt_count in first_attempt..=(BroadcastAttemptCount::<T, I>::take(broadcast_id)) {
			AwaitingBroadcast::<T, I>::remove(BroadcastAttemptId { broadcast_id, attempt_count });
		}

		RequestCallbacks::<T, I>::remove(broadcast_id);
		ThresholdSignatureData::<T, I>::remove(broadcast_id);
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

	/// Request a threshold signature, providing [Call::on_signature_ready] as the callback.
	pub fn threshold_sign_and_broadcast(
		api_call: <T as Config<I>>::ApiCall,
		maybe_callback: Option<<T as Config<I>>::BroadcastCallable>,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		let broadcast_id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});
		if let Some(callback) = maybe_callback {
			RequestCallbacks::<T, I>::insert(broadcast_id, callback);
		}
		let threshold_signature_payload = api_call.threshold_signature_payload();
		let signature_request_id = T::ThresholdSigner::request_signature_with_callback(
			threshold_signature_payload.clone(),
			|threshold_request_id| {
				Call::on_signature_ready {
					threshold_request_id,
					threshold_signature_payload,
					api_call: Box::new(api_call),
					broadcast_id,
				}
				.into()
			},
		);
		(broadcast_id, signature_request_id)
	}

	/// Begin the process of broadcasting a transaction.
	///
	/// ## Events
	///
	/// - [TransactionBroadcastRequest](Event::TransactionBroadcastRequest)
	fn start_broadcast(
		signature: &ThresholdSignatureFor<T, I>,
		transaction_payload: TransactionFor<T, I>,
		api_call: <T as Config<I>>::ApiCall,
		threshold_signature_payload: <T::TargetChain as ChainCrypto>::Payload,
		broadcast_id: BroadcastId,
	) -> BroadcastAttemptId {
		let transaction_out_id = api_call.transaction_out_id();

		T::BroadcastReadyProvider::on_broadcast_ready(&api_call);

		TransactionOutIdToBroadcastId::<T, I>::insert(&transaction_out_id, broadcast_id);

		ThresholdSignatureData::<T, I>::insert(broadcast_id, (api_call, signature));

		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id, attempt_count: 0 };
		Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
			broadcast_attempt_id,
			transaction_payload,
			threshold_signature_payload,
			transaction_out_id,
		});
		broadcast_attempt_id
	}

	fn start_next_broadcast_attempt(broadcast_attempt: BroadcastAttempt<T, I>) {
		let broadcast_id = broadcast_attempt.broadcast_attempt_id.broadcast_id;

		if let Some((api_call, signature)) = ThresholdSignatureData::<T, I>::get(broadcast_id) {
			let EpochKey { key, .. } = T::KeyProvider::active_epoch_key()
				.expect("Epoch key must exist if we made a broadcast.");

			if T::TransactionBuilder::is_valid_for_rebroadcast(
				&api_call,
				&broadcast_attempt.threshold_signature_payload,
				&key,
				&signature,
			) {
				let next_broadcast_attempt_id =
					broadcast_attempt.broadcast_attempt_id.next_attempt();

				BroadcastAttemptCount::<T, I>::mutate(broadcast_id, |attempt_count| {
					*attempt_count += 1;
					*attempt_count
				});
				debug_assert_eq!(
					BroadcastAttemptCount::<T, I>::get(broadcast_id),
					next_broadcast_attempt_id.attempt_count,
				);

				Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
					broadcast_attempt_id: next_broadcast_attempt_id,
					..broadcast_attempt
				});
			}
			// If the signature verification fails, we want
			// to retry from the threshold signing stage.
			else {
				Self::clean_up_broadcast_storage(broadcast_id);
				Self::threshold_sign_and_broadcast(
					api_call,
					RequestCallbacks::<T, I>::get(broadcast_id),
				);
				log::info!(
					"Signature is invalid -> rescheduled threshold signature for broadcast id {}.",
					broadcast_id
				);
				Self::deposit_event(Event::<T, I>::ThresholdSignatureInvalid { broadcast_id });
			}
		} else {
			log::error!("No threshold signature data is available.");
		};
	}

	fn start_broadcast_attempt(mut broadcast_attempt: BroadcastAttempt<T, I>) {
		T::TransactionBuilder::refresh_unsigned_data(&mut broadcast_attempt.transaction_payload);

		let seed =
			(broadcast_attempt.broadcast_attempt_id, broadcast_attempt.transaction_payload.clone())
				.encode();
		if let Some(nominated_signer) = T::BroadcastSignerNomination::nomination_with_seed(
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
			const FAILED_SIGNER_SELECTION: &str = "Failed to select signer: We should either: a) have a signer eligible for nomination b) already have aborted this broadcast when scheduling the retry";
			log::error!("{FAILED_SIGNER_SELECTION}");
			#[cfg(test)]
			panic!("{FAILED_SIGNER_SELECTION}");
		}
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	type Callback = <T as Config<I>>::BroadcastCallable;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		Self::threshold_sign_and_broadcast(api_call, None)
	}

	fn threshold_sign_and_broadcast_with_callback(
		api_call: Self::ApiCall,
		callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		Self::threshold_sign_and_broadcast(api_call, Some(callback))
	}
}
