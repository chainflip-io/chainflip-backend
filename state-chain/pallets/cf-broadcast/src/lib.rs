#![cfg_attr(not(feature = "std"), no_std)]
// This can be removed after rustc version 1.53.
#![feature(extended_key_value_attributes)] // NOTE: This is stable as of rustc v1.54.0
#![doc = include_str!("../README.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use cf_chains::Chain;
use cf_traits::{offline_conditions::*, Chainflip, SignerNomination};
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchResultWithPostInfo, traits::Get, Parameter, Twox64Concat};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

/// The reasons for which a broadcast might fail.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum TransmissionFailure {
	/// The transaction was rejected because of some user error, for example, insuffient funds.
	TransactionRejected,
	/// The transaction failed for some unknown reason and we don't know how to recover.
	TransactionFailed,
}

/// The [BroadcastConfig] should contain all the state required to construct and process transactions for a given
/// chain.
pub trait BroadcastConfig<T: Chainflip> {
	/// A chain identifier.
	type Chain: Chain;
	/// An unsigned version of the transaction that needs to signed before it can be broadcast.
	type UnsignedTransaction: Parameter;
	/// A transaction that has been signed by some account and is ready to be broadcast.
	type SignedTransaction: Parameter;
	/// The transaction hash type used to uniquely identify signed transactions.
	type TransactionHash: Parameter;

	/// Verify the signed transaction when it is submitted to the state chain by the nominated signer.
	///
	/// 'Verification' here is loosely defined as whatever is deemed necessary to accept the validaty of the
	/// returned transaction for this `Chain` and can include verification of the byte encoding, the transaction
	/// content, metadata, signer idenity, etc.
	fn verify_transaction(
		signer: &T::ValidatorId,
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
	) -> Option<()>;
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
	use frame_support::{ensure, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// Type alias for the instance's configured SignedTransaction.
	pub type SignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::SignedTransaction;

	/// Type alias for the instance's configured UnsignedTransaction.
	pub type UnsignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::UnsignedTransaction;

	/// Type alias for the instance's configured TransactionHash.
	pub type TransactionHashFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::TransactionHash;

	/// The first step in the process - a transaction signing attempt.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub nominee: T::ValidatorId,
	}

	/// The second step in the process - the transaction is already signed, it needs to be broadcast.
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
	/// Implements `From` for both [BroadcastAttempt] and [TransactionSigningAttempt] for easy conversion.
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

		/// A marker trait identifying the chain that we are broadcasting to.
		type TargetChain: Chain;

		/// The broadcast configuration for this instance.
		type BroadcastConfig: BroadcastConfig<Self, Chain = Self::TargetChain>;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;

		/// For reporting bad actors.
		type OfflineReporter: OfflineReporter<ValidatorId = Self::ValidatorId>;

		/// The timeout duration for the signing stage, measured in number of blocks.
		#[pallet::constant]
		type SigningTimeout: Get<BlockNumberFor<Self>>;

		/// The timeout duration for the transmission stage, measured in number of blocks.
		#[pallet::constant]
		type TransmissionTimeout: Get<BlockNumberFor<Self>>;
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

	/// Live transaction transmission requests.
	#[pallet::storage]
	pub type AwaitingTransmission<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastAttemptId, TransmissionAttempt<T, I>, OptionQuery>;

	/// The list of failed broadcasts pending retry.
	#[pallet::storage]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FailedBroadcastAttempt<T, I>>, ValueQuery>;

	/// A mapping from block number to a list of signing or broadcast attempts that expire at that block number.
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
		/// A request to a specific validator to sign a transaction. [broadcast_attempt_id, validator_id, unsigned_tx]
		TransactionSigningRequest(
			BroadcastAttemptId,
			T::ValidatorId,
			UnsignedTransactionFor<T, I>,
		),
		/// A request to transmit a signed transaction to the target chain. [broadcast_attempt_id, signed_tx]
		TransmissionRequest(BroadcastAttemptId, SignedTransactionFor<T, I>),
		/// A broadcast has successfully been completed. [broadcast_id]
		BroadcastComplete(BroadcastId),
		/// A failed broadcast attempt has been scheduled for retry. [broadcast_id, attempt]
		BroadcastRetryScheduled(BroadcastId, AttemptCount),
		/// A broadcast has failed irrecoverably. [broadcast_id, attempt, failed_transaction]
		BroadcastFailed(BroadcastId, AttemptCount, UnsignedTransactionFor<T, I>),
		/// A broadcast attempt expired either at the transaction signing stage or the transmission stage. [broadcast_attempt_id, stage]
		BroadcastAttemptExpired(BroadcastAttemptId, BroadcastStage),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided broadcast attempt id is invalid.
		InvalidBroadcastAttemptId,
		/// The transaction signer is not signer who was nominated.
		InvalidSigner,
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
					}
					BroadcastStage::Transmission => {
						if let Some(attempt) = AwaitingTransmission::<T, I>::take(attempt_id) {
							notify_and_retry(attempt.into());
						}
					}
				}
			}

			// TODO: replace this with benchmark results.
			retry_count as u64
				* frame_support::weights::RuntimeDbWeight::default().reads_writes(3, 3)
				+ expiries.len() as u64
					* frame_support::weights::RuntimeDbWeight::default().reads_writes(1, 1)
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Begin the process of broadcasting a transaction.
		///
		/// This triggers the first step - requesting a transaction signature from a nominated validator.
		///
		/// ## Events
		///
		/// - [TransactionSigningRequest](Events::TransactionSigningRequest): Signing has been requested
		///   from the nominated Validator.
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(10_000)]
		pub fn start_broadcast(
			origin: OriginFor<T>,
			unsigned_tx: UnsignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			// TODO: This doesn't necessarily have to be witnessed, but *should* be restricted such that it can only
			// be called internally.
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			let broadcast_id = BroadcastIdCounter::<T, I>::mutate(|id| {
				*id += 1;
				*id
			});

			Self::start_broadcast_attempt(broadcast_id, 0, unsigned_tx);

			Ok(().into())
		}

		/// Called by the nominated signer when they have completed and signed the transaction, and it is therefore ready
		/// to be transmitted. The signed transaction is stored on-chain so that any node can potentially transmit it to
		/// the target chain. Emits an event that will trigger the transmission to the target chain.
		///
		/// ## Events
		///
		/// - [TransmissionRequest](Events::TransmissionRequest): Signed transaction should now be broadcast to the
		///   outgoing chain by all Validators.
		/// - [BroadcastRetryScheduled](Events::BroadcastRetryScheduled): Signed transaction is not valid, so we have
		///   scheduled a retry attempt for the next block.
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Errors::InvalidBroadcastAttemptId): There is no broadcast for this attempt_id.
		/// - [InvalidSigner](Errors::InvalidSigner): Submitter of this extrinsic is not the nominated Validator for this
		///   attempt_id.
		///
		#[pallet::weight(10_000)]
		pub fn transaction_ready_for_transmission(
			origin: OriginFor<T>,
			attempt_id: BroadcastAttemptId,
			signed_tx: SignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			let signing_attempt = AwaitingTransactionSignature::<T, I>::get(attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			ensure!(
				signing_attempt.nominee == signer.into(),
				Error::<T, I>::InvalidSigner
			);

			AwaitingTransactionSignature::<T, I>::remove(attempt_id);

			if T::BroadcastConfig::verify_transaction(
				&signing_attempt.nominee,
				&signing_attempt.unsigned_tx,
				&signed_tx,
			)
			.is_some()
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
				Self::report_and_schedule_retry(
					&signing_attempt.nominee.clone(),
					signing_attempt.into(),
				)
			}

			Ok(().into())
		}

		/// Nodes have witnessed that the transaction has reached finality on the target chain.
		///
		/// ## Events
		///
		/// - [BroadcastComplete](Event::BroadcastComplete): The broadcast to the target chain was successful.
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttmemptId](Error::InvalidBroadcastAttemptId): The attempt id was not in the
		///   queue of broadcasts awaiting transmission.
		#[pallet::weight(10_000)]
		pub fn transmission_success(
			origin: OriginFor<T>,
			attempt_id: BroadcastAttemptId,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			// Remove the transmission details now the broadcast is completed.
			let TransmissionAttempt::<T, I> { broadcast_id, .. } =
				AwaitingTransmission::<T, I>::take(attempt_id)
					.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			Self::deposit_event(Event::<T, I>::BroadcastComplete(broadcast_id));

			Ok(().into())
		}

		/// Nodes have witnessed that something went wrong during transmission. See [BroadcastFailure] for categories
		/// of failures that may be reported.
		///
		/// ## Events
		///
		/// - [BroadcastFailed](Event::BroadcastFailed): The broadcast failed.
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttmemptId](Error::InvalidBroadcastAttemptId): The attempt id was not in the
		///   queue of broadcasts awaiting transmission.
		#[pallet::weight(10_000)]
		pub fn transmission_failure(
			origin: OriginFor<T>,
			attempt_id: BroadcastAttemptId,
			failure: TransmissionFailure,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			let failed_attempt = AwaitingTransmission::<T, I>::take(attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			match failure {
				TransmissionFailure::TransactionRejected => {
					Self::report_and_schedule_retry(
						&failed_attempt.signer.clone(),
						failed_attempt.into(),
					);
				}
				TransmissionFailure::TransactionFailed => {
					Self::deposit_event(Event::<T, I>::BroadcastFailed(
						failed_attempt.broadcast_id,
						failed_attempt.attempt_count,
						failed_attempt.unsigned_tx,
					));
				}
			};

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
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

		// Select a signer for this broadcast.
		let nominated_signer = T::SignerNomination::nomination_with_seed(attempt_id);

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
	}

	fn report_and_schedule_retry(signer: &T::ValidatorId, failed: FailedBroadcastAttempt<T, I>) {
		// TODO: set a sensible penalty and centralise. See #569
		const PENALTY: i32 = 0;
		T::OfflineReporter::report(OfflineCondition::ParticipateSigningFailed, PENALTY, signer)
			.unwrap_or_else(|_| {
				// Should never fail unless the validator doesn't exist.
				frame_support::debug::error!("Unable to report unknown validator {:?}", signer);
				0
			});
		Self::schedule_retry(failed);
	}

	/// Schedule a failed attempt for retry when the next block is authored.
	fn schedule_retry(failed: FailedBroadcastAttempt<T, I>) {
		BroadcastRetryQueue::<T, I>::append(&failed);
		Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled(
			failed.broadcast_id,
			failed.attempt_count,
		));
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
