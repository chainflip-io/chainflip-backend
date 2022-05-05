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
use cf_traits::{
	offence_reporting::OffenceReporter, Broadcaster, Chainflip, SignerNomination, ThresholdSigner,
};
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
	pub broadcast_id: BroadcastId,
	pub attempt_count: AttemptCount,
}

impl BroadcastAttemptId {
	/// Increment the attempt count for a particular BroadcastAttemptId
	pub fn next_attempt(&self) -> Self {
		Self { attempt_count: self.attempt_count + 1, ..*self }
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum PalletOffence {
	InvalidTransactionAuthored,
	TransactionFailedOnTransmission,
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

		/// Offences that can be reported in this runtime.
		type Offence: From<PalletOffence>;

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
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

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

	/// Maps a BroadcastId to a list of unresolved broadcast attempt numbers.
	#[pallet::storage]
	pub type BroadcastIdToAttemptNumbers<T, I = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, Vec<AttemptCount>, OptionQuery>;

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

	/// Live transaction transmission requests.
	#[pallet::storage]
	pub type AwaitingTransmission<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastAttemptId, TransmissionAttempt<T, I>, OptionQuery>;

	/// The list of failed broadcasts pending retry.
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

	// TODO: Amount type
	#[pallet::storage]
	pub type SignerTransactionFeeDeficit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, SignerIdFor<T, I>, u128, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A request to a specific authority to sign a transaction. \[broadcast_attempt_id,
		/// validator_id, unsigned_tx\]
		TransactionSigningRequest(BroadcastAttemptId, T::ValidatorId, UnsignedTransactionFor<T, I>),
		/// A request to transmit a signed transaction to the target chain. \[broadcast_attempt_id,
		/// signed_tx\]
		TransmissionRequest(BroadcastAttemptId, SignedTransactionFor<T, I>),
		/// A broadcast has successfully been completed. \[broadcast_attempt_id\]
		BroadcastComplete(BroadcastAttemptId),
		/// A failed broadcast attempt has been scheduled for retry. \[broadcast_attempt_id\]
		BroadcastRetryScheduled(BroadcastAttemptId),
		/// A broadcast has failed irrecoverably. \[broadcast_attempt_id, failed_transaction\]
		BroadcastFailed(BroadcastAttemptId, UnsignedTransactionFor<T, I>),
		/// A broadcast attempt expired either at the transaction signing stage or the transmission
		/// stage. \[broadcast_attempt_id, stage\]
		BroadcastAttemptExpired(BroadcastAttemptId, BroadcastStage),
		/// A broadcast has been aborted after failing `MaximumAttempts`. \[broadcast_id\]
		BroadcastAborted(BroadcastId),
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
		/// The `on_initialize` hook for this pallet handles scheduled retries and expiries.
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let retries = BroadcastRetryQueue::<T, I>::take();
			let retry_count = retries.len();
			for failed in retries {
				Self::start_next_broadcast_attempt(failed);
			}

			let expiries = Expiries::<T, I>::take(block_number);
			for (stage, attempt_id) in expiries.iter() {
				let notify_and_retry = |attempt: BroadcastAttempt<T, I>| {
					Self::deposit_event(Event::<T, I>::BroadcastAttemptExpired(
						*attempt_id,
						*stage,
					));
					// retry
					Self::start_next_broadcast_attempt(attempt);
				};

				match stage {
					BroadcastStage::TransactionSigning => {
						// We take here. We only allow a single transaction signature request
						// to be valid at a time
						if let Some(signing_attempt) =
							AwaitingTransactionSignature::<T, I>::take(attempt_id)
						{
							// invalidate the old attempt count by removing it from the mapping
							BroadcastIdToAttemptNumbers::<T, I>::mutate(
								signing_attempt.broadcast_attempt.broadcast_attempt_id.broadcast_id,
								|attempt_numbers| {
									if let Some(attempt_numbers) = attempt_numbers {
										attempt_numbers.retain(|x| {
											*x != signing_attempt
												.broadcast_attempt
												.broadcast_attempt_id
												.attempt_count
										});
									}
								},
							);
							notify_and_retry(signing_attempt.broadcast_attempt);
						}
					},
					// when we retry we actually don't want to take the attempt or the count
					BroadcastStage::Transmission => {
						if let Some(transmission_attempt) =
							AwaitingTransmission::<T, I>::get(attempt_id)
						{
							notify_and_retry(transmission_attempt.broadcast_attempt);
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

		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T, I>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T, I>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T, I>::post_upgrade()
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

			let signing_attempt = AwaitingTransactionSignature::<T, I>::get(broadcast_attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			ensure!(signing_attempt.nominee == signer.into(), Error::<T, I>::InvalidSigner);

			// it's no longer being signed, it's being broadcast
			AwaitingTransactionSignature::<T, I>::remove(broadcast_attempt_id);

			if T::TargetChain::verify_signed_transaction(
				&signing_attempt.broadcast_attempt.unsigned_tx,
				&signed_tx,
				&signer_id,
			)
			.is_ok()
			{
				// Whitelist the signer_id so it can receive fee refunds
				if !SignerTransactionFeeDeficit::<T, I>::contains_key(signer_id.clone()) {
					SignerTransactionFeeDeficit::<T, I>::insert(signer_id.clone(), 0);
				}

				AwaitingTransmission::<T, I>::insert(
					broadcast_attempt_id,
					TransmissionAttempt {
						broadcast_attempt: signing_attempt.broadcast_attempt,
						signer: signing_attempt.nominee,
						signed_tx: signed_tx.clone(),
					},
				);
				Self::deposit_event(Event::<T, I>::TransmissionRequest(
					broadcast_attempt_id,
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
					PalletOffence::InvalidTransactionAuthored,
				)
			}

			Ok(().into())
		}

		/// Nodes have witnessed that something went wrong during transmission. See
		/// [BroadcastFailed](Event::BroadcastFailed) for categories of failures that may be
		/// reported.
		/// If this fails
		///
		/// ## Events
		///
		/// - [BroadcastFailed](Event::BroadcastFailed)
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Error::InvalidBroadcastAttemptId)
		/// - [InvalidBroadcastId](Error::InvalidBroadcastId)
		#[pallet::weight(T::WeightInfo::transmission_failure())]
		pub fn transmission_failure(
			origin: OriginFor<T>,
			broadcast_attempt_id: BroadcastAttemptId,
			failure: TransmissionFailure,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureWitnessed::ensure_origin(origin)?;

			let TransmissionAttempt { broadcast_attempt, signer, .. } =
				AwaitingTransmission::<T, I>::take(broadcast_attempt_id)
					.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			// remove this broadcast attempt from the list of attempts for this broadcast
			// and return the latest attempt number
			let last_attempt_number = BroadcastIdToAttemptNumbers::<T, I>::try_mutate(
				broadcast_attempt.broadcast_attempt_id.broadcast_id,
				|attempt_numbers| {
					attempt_numbers
						.as_mut()
						.and_then(|attempt_numbers| {
							let last_attempt = attempt_numbers.last().copied();
							attempt_numbers.retain(|x| *x != broadcast_attempt_id.attempt_count);
							last_attempt
						})
						.ok_or(Error::<T, I>::InvalidBroadcastId)
				},
			)?;

			// if not the latest attempt id, then we should ignore it, because we've
			// already scheduled a retry for it.
			if broadcast_attempt_id.attempt_count != last_attempt_number {
				log::debug!(
					"Ignoring failure for broadcast attempt id {} because it is not the latest attempt",
					broadcast_attempt_id
				);
			} else {
				match failure {
					TransmissionFailure::TransactionRejected => {
						Self::report_and_schedule_retry(
							&signer,
							broadcast_attempt,
							PalletOffence::TransactionFailedOnTransmission,
						);
					},
					TransmissionFailure::TransactionFailed => {
						Self::deposit_event(Event::<T, I>::BroadcastFailed(
							broadcast_attempt.broadcast_attempt_id,
							broadcast_attempt.unsigned_tx,
						));
					},
				};
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
		///
		/// ## Errors
		///
		/// - [InvalidPayload](Event::InvalidPayload)
		#[pallet::weight(T::WeightInfo::signature_accepted())]
		pub fn signature_accepted(
			origin: OriginFor<T>,
			payload: ThresholdSignatureFor<T, I>,
			_tx_signer: SignerIdFor<T, I>,
			_block_number: u64,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			let broadcast_id = SignatureToBroadcastIdLookup::<T, I>::take(payload)
				.ok_or(Error::<T, I>::InvalidPayload)?;

			// Here we need to be able to get the accurate broadcast id from the payload
			let attempt_numbers = BroadcastIdToAttemptNumbers::<T, I>::take(broadcast_id)
				.ok_or(Error::<T, I>::InvalidBroadcastId)?;
			for attempt_count in &attempt_numbers {
				let broadcast_attempt_id =
					BroadcastAttemptId { broadcast_id, attempt_count: *attempt_count };

				// A particular attempt is either alive because at the signing stage
				// OR it's at the transmission stage
				if AwaitingTransmission::<T, I>::take(broadcast_attempt_id).is_none() &&
					AwaitingTransactionSignature::<T, I>::take(broadcast_attempt_id).is_none()
				{
					log::warn!("Attempt {} exists that is neither awaiting sig, nor awaiting transmissions. This should be impossible.", broadcast_attempt_id);
				}
			}
			if let Some(attempt_count) = attempt_numbers.last() {
				let last_broadcast_attempt_id =
					BroadcastAttemptId { broadcast_id, attempt_count: *attempt_count };
				Self::deposit_event(Event::<T, I>::BroadcastComplete(last_broadcast_attempt_id));
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
		BroadcastIdToAttemptNumbers::<T, I>::insert(broadcast_id, vec![0]);

		Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
			broadcast_attempt_id: BroadcastAttemptId { broadcast_id, attempt_count: 0 },
			unsigned_tx,
		});
	}

	fn start_next_broadcast_attempt(broadcast_attempt: BroadcastAttempt<T, I>) {
		let next_broadcast_attempt_id = broadcast_attempt.broadcast_attempt_id.next_attempt();

		BroadcastIdToAttemptNumbers::<T, I>::append(
			broadcast_attempt.broadcast_attempt_id.broadcast_id,
			next_broadcast_attempt_id.attempt_count,
		);

		Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
			broadcast_attempt_id: next_broadcast_attempt_id,
			..broadcast_attempt
		})
	}

	fn start_broadcast_attempt(broadcast_attempt: BroadcastAttempt<T, I>) {
		// Seed based on the input data of the extrinsic
		let seed = (broadcast_attempt.broadcast_attempt_id, broadcast_attempt.unsigned_tx.clone())
			.encode();

		// Check if there is an nominated signer
		if let Some(nominated_signer) = T::SignerNomination::nomination_with_seed(seed) {
			// write, or overwrite the old entry if it exists (on a retry)
			AwaitingTransactionSignature::<T, I>::insert(
				broadcast_attempt.broadcast_attempt_id,
				TransactionSigningAttempt {
					broadcast_attempt: BroadcastAttempt::<T, I> {
						unsigned_tx: broadcast_attempt.unsigned_tx.clone(),
						..broadcast_attempt
					},
					nominee: nominated_signer.clone(),
				},
			);

			// Schedule expiry.
			let expiry_block = frame_system::Pallet::<T>::block_number() + T::SigningTimeout::get();
			Expiries::<T, I>::append(
				expiry_block,
				(BroadcastStage::TransactionSigning, broadcast_attempt.broadcast_attempt_id),
			);

			// Emit the transaction signing request.
			Self::deposit_event(Event::<T, I>::TransactionSigningRequest(
				broadcast_attempt.broadcast_attempt_id,
				nominated_signer,
				broadcast_attempt.unsigned_tx,
			));
		} else {
			// In this case all validators are currently offline. We just do
			// nothing in this case and wait until someone comes up again.
			log::warn!("No online validators at the moment.");
			Self::schedule_retry(broadcast_attempt);
		}
	}

	fn report_and_schedule_retry(
		signer: &T::ValidatorId,
		failed_broadcast_attempt: BroadcastAttempt<T, I>,
		offence: PalletOffence,
	) {
		T::OffenceReporter::report(offence, signer.clone());
		Self::schedule_retry(failed_broadcast_attempt);
	}

	/// Schedule a failed attempt for retry when the next block is authored.
	/// We will abort the broadcast once we have met the attempt threshold `MaximumAttempts`
	fn schedule_retry(failed_broadcast_attempt: BroadcastAttempt<T, I>) {
		if failed_broadcast_attempt.broadcast_attempt_id.attempt_count < T::MaximumAttempts::get() {
			BroadcastRetryQueue::<T, I>::append(&failed_broadcast_attempt);
			Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled(
				failed_broadcast_attempt.broadcast_attempt_id,
			));
		} else {
			Self::deposit_event(Event::<T, I>::BroadcastAborted(
				failed_broadcast_attempt.broadcast_attempt_id.broadcast_id,
			));
		}
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		Self::threshold_sign_and_broadcast(api_call)
	}
}
