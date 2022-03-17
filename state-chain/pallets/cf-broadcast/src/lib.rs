#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use cf_chains::{ApiCall, ChainAbi, ChainCrypto, TransactionBuilder};
use cf_traits::{offline_conditions::*, Broadcaster, Chainflip, SignerNomination, ThresholdSigner};
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchResultWithPostInfo, traits::Get, Twox64Concat};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::{marker::PhantomData, prelude::*};

/// The reasons for which a broadcast might fail.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum TransmissionFailure {
	/// The transaction was rejected because of some user error, for example, insuffient funds.
	TransactionRejected,
	/// The transaction failed for some unknown reason and we don't know how to recover.
	TransactionFailed,
}

/// A unique id for each broadcast attempt.
pub type BroadcastAttemptId = u64;

/// A unique id for each broadcast.
pub type BroadcastId = u32;

/// The number of broadcast attempts that were made before this one.
pub type AttemptCount = u32;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{ensure, pallet_prelude::*, traits::EnsureOrigin};
	use frame_system::pallet_prelude::*;

	/// Type alias for the instance's configured SignedTransaction.
	pub type SignedTransactionFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainAbi>::SignedTransaction;

	/// Type alias for the instance's configured UnsignedTransaction.
	pub type UnsignedTransactionFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainAbi>::UnsignedTransaction;

	/// Type alias for the instance's configured TransactionHash.
	pub type TransactionHashFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainCrypto>::TransactionHash;

	/// Type alias for the instance's configured SignerId.
	pub type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignerCredential;

	/// Type alias for the payload hash
	pub type ThresholdSignatureFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

	/// The first step in the process - a transaction signing attempt.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub nominee: T::ValidatorId,
	}

	/// The second step in the process - the transaction is already signed, it needs to be
	/// broadcast.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransmissionAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub signer: T::ValidatorId,
		pub signed_tx: SignedTransactionFor<T, I>,
	}

	/// A failed signing or broadcasting attempt.
	///
	/// Implements `From` for both [TransmissionAttempt] and [TransactionSigningAttempt] for easy
	/// conversion.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct FailedBroadcastAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
	}

	impl<T: Config<I>, I: 'static> From<TransmissionAttempt<T, I>> for FailedBroadcastAttempt<T, I> {
		fn from(failed: TransmissionAttempt<T, I>) -> Self {
			Self {
				broadcast_id: failed.broadcast_id,
				attempt_count: failed.attempt_count,
				unsigned_tx: failed.unsigned_tx,
			}
		}
	}

	impl<T: Config<I>, I: 'static> From<TransactionSigningAttempt<T, I>>
		for FailedBroadcastAttempt<T, I>
	{
		fn from(failed: TransactionSigningAttempt<T, I>) -> Self {
			Self {
				broadcast_id: failed.broadcast_id,
				attempt_count: failed.attempt_count,
				unsigned_tx: failed.unsigned_tx,
			}
		}
	}

	/// For tagging the signing or transmission stage of the broadcast
	#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub enum BroadcastStage {
		TransactionSigning,
		Transmission,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type Call: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::Call>;

		/// A marker trait identifying the chain that we are broadcasting to.
		type TargetChain: ChainAbi;

		/// The api calls supported by this broadcaster.
		type ApiCall: ApiCall<Self::TargetChain>;

		/// Builds the transaction according to the chain's environment settings.
		type TransactionBuilder: TransactionBuilder<Self::TargetChain, Self::ApiCall>;

		/// A threshold signer that can sign calls for this chain, and dispatch callbacks into this
		/// pallet.
		type ThresholdSigner: ThresholdSigner<
			Self::TargetChain,
			Callback = <Self as Config<I>>::Call,
		>;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;

		/// For reporting bad actors.
		type OfflineReporter: OfflineReporter<ValidatorId = Self::ValidatorId>;

		/// Ensure that only threshold signature consensus can trigger a broadcast.
		type EnsureThresholdSigned: EnsureOrigin<Self::Origin>;

		/// The timeout duration for the signing stage, measured in number of blocks.
		#[pallet::constant]
		type SigningTimeout: Get<BlockNumberFor<Self>>;

		/// The timeout duration for the transmission stage, measured in number of blocks.
		#[pallet::constant]
		type TransmissionTimeout: Get<BlockNumberFor<Self>>;

		/// Maximum number of attempts
		#[pallet::constant]
		type MaximumAttempts: Get<AttemptCount>;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter for incrementing the broadcast attempt id.
	#[pallet::storage]
	pub type BroadcastAttemptIdCounter<T, I = ()> = StorageValue<_, BroadcastAttemptId, ValueQuery>;

	/// A counter for incrementing the broadcast id.
	#[pallet::storage]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	/// Live transaction signing requests.
	#[pallet::storage]
	pub type AwaitingTransactionSignature<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastAttemptId,
		TransactionSigningAttempt<T, I>,
		OptionQuery,
	>;

	/// Lookup table between Signature -> Broadcast
	#[pallet::storage]
	pub type SignatureToBroadcastIdLookup<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, ThresholdSignatureFor<T, I>, BroadcastId, OptionQuery>;

	/// Lookup table between BroadcastId -> AttemptId
	#[pallet::storage]
	pub type BroadcastIdToAttemptIdLookup<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, BroadcastAttemptId, OptionQuery>;

	/// Live transaction transmission requests.
	#[pallet::storage]
	pub type AwaitingTransmission<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastAttemptId, TransmissionAttempt<T, I>, OptionQuery>;

	/// The list of failed broadcasts pending retry.
	#[pallet::storage]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FailedBroadcastAttempt<T, I>>, ValueQuery>;

	/// A mapping from block number to a list of signing or broadcast attempts that expire at that
	/// block number.
	#[pallet::storage]
	pub type Expiries<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Vec<(BroadcastStage, BroadcastAttemptId)>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A request to a specific validator to sign a transaction. \[broadcast_attempt_id,
		/// validator_id, unsigned_tx\]
		TransactionSigningRequest(BroadcastAttemptId, T::ValidatorId, UnsignedTransactionFor<T, I>),
		/// A request to transmit a signed transaction to the target chain. \[broadcast_attempt_id,
		/// signed_tx\]
		TransmissionRequest(BroadcastAttemptId, SignedTransactionFor<T, I>),
		/// A broadcast has successfully been completed. \[broadcast_id\]
		BroadcastComplete(BroadcastId),
		/// A failed broadcast attempt has been scheduled for retry. \[broadcast_id, attempt\]
		BroadcastRetryScheduled(BroadcastId, AttemptCount),
		/// A broadcast has failed irrecoverably. \[broadcast_id, attempt, failed_transaction\]
		BroadcastFailed(BroadcastId, AttemptCount, UnsignedTransactionFor<T, I>),
		/// A broadcast attempt expired either at the transaction signing stage or the transmission
		/// stage. \[broadcast_attempt_id, stage\]
		BroadcastAttemptExpired(BroadcastAttemptId, BroadcastStage),
		/// A broadcast has been aborted after failing `MaximumAttempts`. \[broadcast_id\]
		BroadcastAborted(BroadcastId),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided broadcast attempt id is invalid.
		InvalidBroadcastAttemptId,
		/// The transaction signer is not signer who was nominated.
		InvalidSigner,
		/// A threshold signature was expected but not available.
		ThresholdSignatureUnavailable,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// The `on_initialize` hook for this pallet handles scheduled retries and expiries.
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let retries = BroadcastRetryQueue::<T, I>::take();
			let retry_count = retries.len();
			for failed in retries {
				Self::retry_failed_broadcast(failed);
			}

			let expiries = Expiries::<T, I>::take(block_number);
			for (stage, attempt_id) in expiries.iter() {
				let notify_and_retry = |attempt: FailedBroadcastAttempt<T, I>| {
					Self::deposit_event(Event::<T, I>::BroadcastAttemptExpired(
						*attempt_id,
						*stage,
					));
					Self::retry_failed_broadcast(attempt);
				};

				match stage {
					BroadcastStage::TransactionSigning => {
						if let Some(attempt) =
							AwaitingTransactionSignature::<T, I>::take(attempt_id)
						{
							notify_and_retry(attempt.into());
						}
					},
					BroadcastStage::Transmission => {
						if let Some(attempt) = AwaitingTransmission::<T, I>::take(attempt_id) {
							notify_and_retry(attempt.into());
						}
					},
				}
			}

			// TODO: replace this with benchmark results.
			retry_count as u64 *
				frame_support::weights::RuntimeDbWeight::default().reads_writes(3, 3) +
				expiries.len() as u64 *
					frame_support::weights::RuntimeDbWeight::default().reads_writes(1, 1)
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Called by the nominated signer when they have completed and signed the transaction, and
		/// it is therefore ready to be transmitted. The signed transaction is stored on-chain so
		/// that any node can potentially transmit it to the target chain. Emits an event that will
		/// trigger the transmission to the target chain.
		///
		/// ## Events
		///
		/// - [TransmissionRequest](Event::TransmissionRequest)
		/// - [BroadcastRetryScheduled](Event::BroadcastRetryScheduled)
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Error::InvalidBroadcastAttemptId)
		/// - [InvalidSigner](Error::InvalidSigner)
		#[pallet::weight(T::WeightInfo::transaction_ready_for_transmission())]
		pub fn transaction_ready_for_transmission(
			origin: OriginFor<T>,
			attempt_id: BroadcastAttemptId,
			signed_tx: SignedTransactionFor<T, I>,
			signer_id: SignerIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			let signing_attempt = AwaitingTransactionSignature::<T, I>::get(attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			ensure!(signing_attempt.nominee == signer.into(), Error::<T, I>::InvalidSigner);

			AwaitingTransactionSignature::<T, I>::remove(attempt_id);

			if T::TargetChain::verify_signed_transaction(
				&signing_attempt.unsigned_tx,
				&signed_tx,
				&signer_id,
			)
			.is_ok()
			{
				Self::deposit_event(Event::<T, I>::TransmissionRequest(
					attempt_id,
					signed_tx.clone(),
				));
				AwaitingTransmission::<T, I>::insert(
					attempt_id,
					TransmissionAttempt {
						broadcast_id: signing_attempt.broadcast_id,
						unsigned_tx: signing_attempt.unsigned_tx,
						signer: signing_attempt.nominee.clone(),
						signed_tx,
						attempt_count: signing_attempt.attempt_count,
					},
				);

				// Schedule expiry.
				let expiry_block =
					frame_system::Pallet::<T>::block_number() + T::TransmissionTimeout::get();
				Expiries::<T, I>::mutate(expiry_block, |entries| {
					entries.push((BroadcastStage::Transmission, attempt_id))
				});
			} else {
				log::warn!("Unable to verify tranaction signature for attempt {}", attempt_id);
				Self::report_and_schedule_retry(
					&signing_attempt.nominee.clone(),
					signing_attempt.into(),
					OfflineCondition::InvalidTransactionAuthored,
				)
			}

			Ok(().into())
		}

		/// Nodes have witnessed that the transaction has reached finality on the target chain.
		///
		/// ## Events
		///
		/// - [BroadcastComplete](Event::BroadcastComplete)
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttmemptId](Error::InvalidBroadcastAttemptId)
		#[pallet::weight(T::WeightInfo::transmission_success())]
		pub fn transmission_success(
			origin: OriginFor<T>,
			attempt_id: BroadcastAttemptId,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureWitnessed::ensure_origin(origin)?;

			// Remove the transmission details now the broadcast is completed.
			let TransmissionAttempt::<T, I> { broadcast_id, .. } =
				AwaitingTransmission::<T, I>::take(attempt_id)
					.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			// Cleanup lookup storage
			Self::remove_lookup_storage(broadcast_id);

			Self::deposit_event(Event::<T, I>::BroadcastComplete(broadcast_id));

			Ok(().into())
		}

		/// Nodes have witnessed that something went wrong during transmission. See
		/// [BroadcastFailed](Event::BroadcastFailed) for categories of failures that may be
		/// reported.
		///
		/// ## Events
		///
		/// - [BroadcastFailed](Event::BroadcastFailed)
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Error::InvalidBroadcastAttemptId)
		#[pallet::weight(T::WeightInfo::transmission_failure())]
		pub fn transmission_failure(
			origin: OriginFor<T>,
			attempt_id: BroadcastAttemptId,
			failure: TransmissionFailure,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureWitnessed::ensure_origin(origin)?;

			let failed_attempt = AwaitingTransmission::<T, I>::take(attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			match failure {
				TransmissionFailure::TransactionRejected => {
					Self::report_and_schedule_retry(
						&failed_attempt.signer.clone(),
						failed_attempt.into(),
						OfflineCondition::TransactionFailedOnTransmission,
					);
				},
				TransmissionFailure::TransactionFailed => {
					Self::deposit_event(Event::<T, I>::BroadcastFailed(
						failed_attempt.broadcast_id,
						failed_attempt.attempt_count,
						failed_attempt.unsigned_tx,
					));
				},
			};

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
			threshold_request_id: <T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId,
			api_call: <T as Config<I>>::ApiCall,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureThresholdSigned::ensure_origin(origin)?;

			let sig =
				T::ThresholdSigner::signature_result(threshold_request_id).ready_or_else(|r| {
					log::error!(
						"Signature not found for threshold request {:?}. Request status: {:?}",
						threshold_request_id,
						r
					);
					Error::<T, I>::ThresholdSignatureUnavailable
				})?;

			Self::start_broadcast(
				&sig,
				T::TransactionBuilder::build_transaction(&api_call.signed(&sig)),
			);

			Ok(().into())
		}

		/// Nodes have witnessed that a signature was accepted on the target chain.
		///
		/// ## Events
		///
		/// - [BroadcastComplete](Event::BroadcastComplete)
		#[pallet::weight(T::WeightInfo::signature_accepted())]
		pub fn signature_accepted(
			origin: OriginFor<T>,
			payload: ThresholdSignatureFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;
			if let Some(broadcast_id) = SignatureToBroadcastIdLookup::<T, I>::take(payload) {
				match BroadcastIdToAttemptIdLookup::<T, I>::take(broadcast_id) {
					Some(attempt_id)
						if AwaitingTransmission::<T, I>::take(attempt_id).is_some() =>
						Self::deposit_event(Event::<T, I>::BroadcastComplete(broadcast_id)),
					_ => (),
				}
			}
			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Request a threshold signature, providing [Call::on_signature_ready] as the callback.
	pub fn threshold_sign_and_broadcast(api_call: <T as Config<I>>::ApiCall) {
		T::ThresholdSigner::request_signature_with_callback(
			api_call.threshold_signature_payload(),
			|id| Call::on_signature_ready(id, api_call).into(),
		);
	}

	/// Begin the process of broadcasting a transaction.
	///
	/// ## Events
	///
	/// - [TransactionSigningRequest](Event::TransactionSigningRequest)
	fn start_broadcast(
		signature: &ThresholdSignatureFor<T, I>,
		unsigned_tx: UnsignedTransactionFor<T, I>,
	) {
		let broadcast_id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		SignatureToBroadcastIdLookup::<T, I>::insert(signature, broadcast_id);

		Self::start_broadcast_attempt(broadcast_id, 0, unsigned_tx);
	}

	// TODO: remove this function when we remove the transmission_success extrinsic
	fn remove_lookup_storage(broadcast_id: BroadcastId) {
		// Remove the BroadcastId lookup
		BroadcastIdToAttemptIdLookup::<T, I>::take(broadcast_id);
		// Try to figure out the payload by the broadcast_id
		if let Some(payload) = SignatureToBroadcastIdLookup::<T, I>::iter()
			.filter_map(|(payload, id)| if id == broadcast_id { Some(payload) } else { None })
			.next()
		{
			// Remove the payload lookup
			SignatureToBroadcastIdLookup::<T, I>::remove(payload);
		}
	}

	fn start_broadcast_attempt(
		broadcast_id: BroadcastId,
		attempt_count: AttemptCount,
		unsigned_tx: UnsignedTransactionFor<T, I>,
	) {
		// Get a new id.
		let attempt_id = BroadcastAttemptIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Update the lookup table
		BroadcastIdToAttemptIdLookup::<T, I>::insert(broadcast_id, attempt_id);

		// Seed based on the input data of the extrinsic
		let seed = (attempt_id, unsigned_tx.clone()).encode();

		// Select a signer for this broadcast.
		let nominated_signer = T::SignerNomination::nomination_with_seed(seed);

		// Check if there is an nominated signer
		if let Some(nominated_signer) = nominated_signer {
			AwaitingTransactionSignature::<T, I>::insert(
				attempt_id,
				TransactionSigningAttempt::<T, I> {
					broadcast_id,
					attempt_count,
					unsigned_tx: unsigned_tx.clone(),
					nominee: nominated_signer.clone(),
				},
			);

			// Schedule expiry.
			let expiry_block = frame_system::Pallet::<T>::block_number() + T::SigningTimeout::get();
			Expiries::<T, I>::mutate(expiry_block, |entries| {
				entries.push((BroadcastStage::TransactionSigning, attempt_id))
			});

			// Emit the transaction signing request.
			Self::deposit_event(Event::<T, I>::TransactionSigningRequest(
				attempt_id,
				nominated_signer,
				unsigned_tx,
			));
		} else {
			// In this case all validators are currently offline. We just do
			// nothing in this case and wait until someone comes up again.
			log::warn!("No online validators at the moment.");
			let failed =
				FailedBroadcastAttempt::<T, I> { broadcast_id, attempt_count, unsigned_tx };
			Self::schedule_retry(failed);
		}
	}

	fn report_and_schedule_retry(
		signer: &T::ValidatorId,
		failed: FailedBroadcastAttempt<T, I>,
		offline_condition: OfflineCondition,
	) {
		T::OfflineReporter::report(offline_condition, signer);
		Self::schedule_retry(failed);
	}

	/// Schedule a failed attempt for retry when the next block is authored.
	/// We will abort the broadcast once we have met the attempt threshold `MaximumAttempts`
	fn schedule_retry(failed: FailedBroadcastAttempt<T, I>) {
		if failed.attempt_count < T::MaximumAttempts::get() {
			BroadcastRetryQueue::<T, I>::append(&failed);
			Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled(
				failed.broadcast_id,
				failed.attempt_count,
			));
		} else {
			Self::deposit_event(Event::<T, I>::BroadcastAborted(failed.broadcast_id));
		}
	}

	/// Retry a failed attempt by starting anew with incremented attempt_count.
	fn retry_failed_broadcast(failed: FailedBroadcastAttempt<T, I>) {
		Self::start_broadcast_attempt(
			failed.broadcast_id,
			failed.attempt_count.wrapping_add(1),
			failed.unsigned_tx,
		);
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		Self::threshold_sign_and_broadcast(api_call)
	}
}
