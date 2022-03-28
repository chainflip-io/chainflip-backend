#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod migrations;

pub mod weights;
pub use weights::WeightInfo;

use cf_chains::{ApiCall, ChainAbi, ChainCrypto, TransactionBuilder};
use cf_traits::{offence_reporting::*, Broadcaster, Chainflip, SignerNomination, ThresholdSigner};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
	Twox64Concat,
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::{marker::PhantomData, prelude::*};

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

/// The reasons for which a broadcast might fail.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum TransmissionFailure {
	/// The transaction was rejected because of some user error, for example, insuffient funds.
	TransactionRejected,
	/// The transaction failed for some unknown reason and we don't know how to recover.
	TransactionFailed,
}

/// A unique id for each broadcast.
pub type BroadcastId = u32;

/// The number of broadcast attempts that were made before this one.
pub type AttemptCount = u32;

/// A unique id for each broadcast attempt
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Default, Copy)]
pub struct BroadcastAttemptId {
	broadcast_id: BroadcastId,
	attempt_count: AttemptCount,
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

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct BroadcastAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt_id: BroadcastAttemptId,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
	}

	/// The first step in the process - a transaction signing attempt.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt: BroadcastAttempt<T, I>,
		pub nominee: T::ValidatorId,
	}

	/// The second step in the process - the transaction is already signed, it needs to be
	/// broadcast.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransmissionAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt: BroadcastAttempt<T, I>,
		pub signer: T::ValidatorId,
		pub signed_tx: SignedTransactionFor<T, I>,
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
		type OffenceReporter: OffenceReporter<ValidatorId = Self::ValidatorId>;

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
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter for incrementing the broadcast id.
	#[pallet::storage]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	/// Live transaction signing requests.
	/// CAN WE USE BROADCAST ID HERE TOO???
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

	/// Live transaction transmission requests.
	#[pallet::storage]
	pub type AwaitingTransmission<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, TransmissionAttempt<T, I>, OptionQuery>;

	/// The list of failed broadcasts pending retry.
	// Why do we need this extra Failed BroadcastAttemptStruct
	#[pallet::storage]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<BroadcastAttempt<T, I>>, ValueQuery>;

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
		/// A failed broadcast attempt has been scheduled for retry. \[broadcast_attempt_id\]
		BroadcastRetryScheduled(BroadcastAttemptId),
		/// A broadcast has failed irrecoverably. \[broadcast_id, attempt, failed_transaction\]
		BroadcastFailed(BroadcastAttemptId, UnsignedTransactionFor<T, I>),
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
				let notify_and_retry = |attempt: BroadcastAttempt<T, I>| {
					Self::deposit_event(Event::<T, I>::BroadcastAttemptExpired(
						attempt_id.clone(),
						*stage,
					));
					Self::retry_failed_broadcast(attempt);
				};

				match stage {
					BroadcastStage::TransactionSigning => {
						if let Some(attempt) =
							AwaitingTransactionSignature::<T, I>::take(attempt_id)
						{
							notify_and_retry(attempt.broadcast_attempt);
						}
					},
					BroadcastStage::Transmission => {
						if let Some(attempt) =
							AwaitingTransmission::<T, I>::take(attempt_id.broadcast_id)
						{
							notify_and_retry(attempt.broadcast_attempt);
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
			broadcast_attempt_id: BroadcastAttemptId,
			signed_tx: SignedTransactionFor<T, I>,
			signer_id: SignerIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			let signing_attempt =
				AwaitingTransactionSignature::<T, I>::get(broadcast_attempt_id.clone())
					.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			ensure!(signing_attempt.nominee == signer.into(), Error::<T, I>::InvalidSigner);

			AwaitingTransactionSignature::<T, I>::remove(broadcast_attempt_id.clone());

			if T::TargetChain::verify_signed_transaction(
				&signing_attempt.broadcast_attempt.unsigned_tx,
				&signed_tx,
				&signer_id,
			)
			.is_ok()
			{
				AwaitingTransmission::<T, I>::insert(
					broadcast_attempt_id.broadcast_id,
					TransmissionAttempt {
						broadcast_attempt: signing_attempt.broadcast_attempt,
						signer: signing_attempt.nominee.clone(),
						signed_tx: signed_tx.clone(),
					},
				);
				Self::deposit_event(Event::<T, I>::TransmissionRequest(
					broadcast_attempt_id.clone(),
					signed_tx,
				));

				// Schedule expiry.
				let expiry_block =
					frame_system::Pallet::<T>::block_number() + T::TransmissionTimeout::get();
				Expiries::<T, I>::mutate(expiry_block, |entries| {
					entries.push((BroadcastStage::Transmission, broadcast_attempt_id))
				});
			} else {
				log::warn!(
					"Unable to verify tranaction signature for broadcast attempt id {}",
					broadcast_attempt_id
				);
				Self::report_and_schedule_retry(
					&signing_attempt.nominee.clone(),
					signing_attempt.broadcast_attempt,
					Offence::InvalidTransactionAuthored,
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
			// TODO: This can be broadcast id
			broadcast_attempt_id: BroadcastAttemptId,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureWitnessed::ensure_origin(origin)?;

			// == Clean up storage items ==
			AwaitingTransmission::<T, I>::take(broadcast_attempt_id.broadcast_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			if let Some(payload) = SignatureToBroadcastIdLookup::<T, I>::iter()
				.filter_map(|(payload, id)| {
					if id == broadcast_attempt_id.broadcast_id {
						Some(payload)
					} else {
						None
					}
				})
				.next()
			{
				SignatureToBroadcastIdLookup::<T, I>::remove(payload);
			}
			// ====

			Self::deposit_event(Event::<T, I>::BroadcastComplete(
				broadcast_attempt_id.broadcast_id,
			));

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
			// TODO: This can just be the BroadcastId
			broadcast_attempt_id: BroadcastAttemptId,
			failure: TransmissionFailure,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureWitnessed::ensure_origin(origin)?;

			let TransmissionAttempt { broadcast_attempt, signer, .. } =
				AwaitingTransmission::<T, I>::take(broadcast_attempt_id.broadcast_id)
					.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			match failure {
				TransmissionFailure::TransactionRejected => {
					Self::report_and_schedule_retry(
						&signer.clone(),
						broadcast_attempt,
						Offence::TransactionFailedOnTransmission,
					);
				},
				TransmissionFailure::TransactionFailed => {
					Self::deposit_event(Event::<T, I>::BroadcastFailed(
						broadcast_attempt.broadcast_attempt_id,
						broadcast_attempt.unsigned_tx,
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

			// Here we need to be able to get the accurate broadcast id from the payload
			if let Some(broadcast_id) = SignatureToBroadcastIdLookup::<T, I>::take(payload) {
				if AwaitingTransmission::<T, I>::take(broadcast_id).is_some() {
					Self::deposit_event(Event::<T, I>::BroadcastComplete(broadcast_id));
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

		// when we take will we always be taking with the same attempt count
		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id, attempt_count: 0 };

		SignatureToBroadcastIdLookup::<T, I>::insert(signature, broadcast_id);

		Self::start_broadcast_attempt(broadcast_attempt_id, unsigned_tx);
	}

	fn start_broadcast_attempt(
		broadcast_attempt_id: BroadcastAttemptId,
		unsigned_tx: UnsignedTransactionFor<T, I>,
	) {
		// increment the attempt
		let next_broadcast_attempt_id = BroadcastAttemptId {
			attempt_count: broadcast_attempt_id.attempt_count + 1,
			broadcast_id: broadcast_attempt_id.broadcast_id,
		};

		// Seed based on the input data of the extrinsic
		let seed = (broadcast_attempt_id.clone(), unsigned_tx.clone()).encode();

		// Select a signer for this broadcast.
		let nominated_signer = T::SignerNomination::nomination_with_seed(seed);

		// Check if there is an nominated signer
		if let Some(nominated_signer) = nominated_signer {
			// instead of inserting, we may want to mutate
			AwaitingTransactionSignature::<T, I>::insert(
				next_broadcast_attempt_id.clone(),
				TransactionSigningAttempt::<T, I> {
					broadcast_attempt: BroadcastAttempt {
						broadcast_attempt_id: next_broadcast_attempt_id,
						unsigned_tx: unsigned_tx.clone(),
					},
					nominee: nominated_signer.clone(),
				},
			);

			// remove the old one
			AwaitingTransactionSignature::<T, I>::remove(broadcast_attempt_id);

			// Schedule expiry.
			let expiry_block = frame_system::Pallet::<T>::block_number() + T::SigningTimeout::get();
			Expiries::<T, I>::mutate(expiry_block, |entries| {
				entries
					.push((BroadcastStage::TransactionSigning, next_broadcast_attempt_id.clone()))
			});

			// Emit the transaction signing request.
			Self::deposit_event(Event::<T, I>::TransactionSigningRequest(
				next_broadcast_attempt_id,
				nominated_signer,
				unsigned_tx,
			));
		} else {
			// In this case all validators are currently offline. We just do
			// nothing in this case and wait until someone comes up again.
			log::warn!("No online validators at the moment.");
			let failed = BroadcastAttempt::<T, I> {
				broadcast_attempt_id: next_broadcast_attempt_id,
				unsigned_tx,
			};
			Self::schedule_retry(failed);
		}
	}

	fn report_and_schedule_retry(
		signer: &T::ValidatorId,
		failed: BroadcastAttempt<T, I>,
		offence: Offence,
	) {
		T::OffenceReporter::report(offence, signer);
		Self::schedule_retry(failed);
	}

	/// Schedule a failed attempt for retry when the next block is authored.
	/// We will abort the broadcast once we have met the attempt threshold `MaximumAttempts`
	fn schedule_retry(failed: BroadcastAttempt<T, I>) {
		if failed.broadcast_attempt_id.attempt_count < T::MaximumAttempts::get() {
			BroadcastRetryQueue::<T, I>::append(&failed);
			Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled(
				failed.broadcast_attempt_id,
			));
		} else {
			Self::deposit_event(Event::<T, I>::BroadcastAborted(
				failed.broadcast_attempt_id.broadcast_id,
			));
		}
	}

	/// Retry a failed attempt by starting anew with incremented attempt_count.
	fn retry_failed_broadcast(failed: BroadcastAttempt<T, I>) {
		// When we retry failed we should increment the storage of the awaiting signatures right?

		Self::start_broadcast_attempt(failed.broadcast_attempt_id, failed.unsigned_tx);
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		Self::threshold_sign_and_broadcast(api_call)
	}
}
