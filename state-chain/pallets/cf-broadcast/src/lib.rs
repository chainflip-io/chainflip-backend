#![cfg_attr(not(feature = "std"), no_std)]
// This can be removed after rustc version 1.53.
#![feature(int_bits_const)]

//! Transaction Broadcast Pallet
//! https://swimlanes.io/u/1s-nyDuYQ

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use cf_chains::Chain;
use cf_traits::{offline_conditions::*, Chainflip, SignerNomination};
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchResultWithPostInfo, Parameter, Twox64Concat};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::marker::PhantomData;
use sp_std::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum BroadcastFailure {
	/// The transaction was rejected because of some user error.
	TransactionRejected,
	/// The transaction failed for some unknown reason.
	TransactionFailed,
	/// The transaction stalled.
	TransactionTimeout,
}

/// The [TransactionContext] should contain all the state required to construct and process transactions for a given
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
	fn verify_transaction(
		signer: &T::ValidatorId,
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
	) -> Option<()>;
}

/// A unique id for each broadcast attempt.
pub type BroadcastId = u64;

/// The number of attempts that were made before this one.
pub type AttemptCount = u32;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{ensure, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	pub type SignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::SignedTransaction;
	pub type UnsignedTransactionFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::UnsignedTransaction;
	pub type TransactionHashFor<T, I> =
		<<T as Config<I>>::BroadcastConfig as BroadcastConfig<T>>::TransactionHash;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct SigningAttempt<T: Config<I>, I: 'static> {
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub nominee: T::ValidatorId,
		pub attempt_count: AttemptCount,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct BroadcastAttempt<T: Config<I>, I: 'static> {
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub signer: T::ValidatorId,
		pub signed_tx: SignedTransactionFor<T, I>,
		pub attempt_count: AttemptCount,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct FailedAttempt<T: Config<I>, I: 'static> {
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub attempt_count: AttemptCount,
	}

	impl<T: Config<I>, I: 'static> From<BroadcastAttempt<T, I>> for FailedAttempt<T, I> {
		fn from(failed: BroadcastAttempt<T, I>) -> Self {
			Self {
				unsigned_tx: failed.unsigned_tx,
				attempt_count: failed.attempt_count,
			}
		}
	}

	impl<T: Config<I>, I: 'static> From<SigningAttempt<T, I>> for FailedAttempt<T, I> {
		fn from(failed: SigningAttempt<T, I>) -> Self {
			Self {
				unsigned_tx: failed.unsigned_tx,
				attempt_count: failed.attempt_count,
			}
		}
	}

	#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub enum SigningOrBroadcast {
		SigningStage,
		BroadcastStage,
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
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	#[pallet::getter(fn request_id_counter)]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	#[pallet::storage]
	pub type AwaitingSignature<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, SigningAttempt<T, I>, OptionQuery>;

	#[pallet::storage]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, BroadcastAttempt<T, I>, OptionQuery>;

	#[pallet::storage]
	pub type RetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FailedAttempt<T, I>>, ValueQuery>;

	#[pallet::storage]
	pub type Expiries<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Vec<(SigningOrBroadcast, BroadcastId)>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// [broadcast_id, validator_id, unsigned_tx]
		TransactionSigningRequest(BroadcastId, T::ValidatorId, UnsignedTransactionFor<T, I>),
		/// [broadcast_id, signed_tx]
		BroadcastRequest(BroadcastId, SignedTransactionFor<T, I>),
		/// [broadcast_id]
		BroadcastComplete(BroadcastId),
		/// [broadcast_id, attempt]
		RetryScheduled(BroadcastId, AttemptCount),
		/// [broadcast_id, attempt, failed_transaction]
		BroadcastFailed(BroadcastId, AttemptCount, UnsignedTransactionFor<T, I>),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided request id is invalid.
		InvalidBroadcastId,
		/// The transaction signer is not signer who was nominated.
		InvalidSigner,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let retries = RetryQueue::<T, I>::take();
			let retry_count = retries.len();
			for request in retries {
				Self::broadcast_attempt(request.unsigned_tx, request.attempt_count.wrapping_add(1));
			}

			let expiries = Expiries::<T, I>::take(block_number);
			for (stage, id) in expiries.iter() {
				match stage {
					SigningOrBroadcast::SigningStage => {
						AwaitingSignature::<T, I>::take(id).map(|signing_attempt| {
							Self::retry(*id, signing_attempt.into());
						});
					}
					SigningOrBroadcast::BroadcastStage => {
						AwaitingBroadcast::<T, I>::take(id).map(|broadcast_attempt| {
							Self::retry(*id, broadcast_attempt.into());
						});
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
		/// This is the first step - requsting a transaction signature from a nominated validator.
		#[pallet::weight(10_000)]
		pub fn start_sign_and_broadcast(
			origin: OriginFor<T>,
			unsigned_tx: UnsignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			Self::broadcast_attempt(unsigned_tx, 0);

			Ok(().into())
		}

		/// Called by the nominated signer when they have completed and signed the transaction, and it is therefore ready
		/// to be broadcast. The signed transaction is stored on-chain so that any node can potentially broadcast it to
		/// the target chain. Emits an event that will trigger the broadcast to the target chain.
		#[pallet::weight(10_000)]
		pub fn transaction_ready(
			origin: OriginFor<T>,
			id: BroadcastId,
			signed_tx: SignedTransactionFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			let signing_attempt =
				AwaitingSignature::<T, I>::get(id).ok_or(Error::<T, I>::InvalidBroadcastId)?;

			ensure!(
				signing_attempt.nominee == signer.into(),
				Error::<T, I>::InvalidSigner
			);

			AwaitingSignature::<T, I>::remove(id);

			if T::BroadcastConfig::verify_transaction(
				&signing_attempt.nominee,
				&signing_attempt.unsigned_tx,
				&signed_tx,
			)
			.is_some()
			{
				Self::deposit_event(Event::<T, I>::BroadcastRequest(id, signed_tx.clone()));
				AwaitingBroadcast::<T, I>::insert(
					id,
					BroadcastAttempt {
						unsigned_tx: signing_attempt.unsigned_tx,
						signer: signing_attempt.nominee.clone(),
						signed_tx,
						attempt_count: signing_attempt.attempt_count,
					},
				);
			} else {
				Self::report_and_retry(id, &signing_attempt.nominee.clone(), signing_attempt.into())
			}

			Ok(().into())
		}

		/// Nodes have witnessed that the transaction has reached finality on the target chain.
		#[pallet::weight(10_000)]
		pub fn broadcast_success(
			origin: OriginFor<T>,
			id: BroadcastId,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			ensure!(
				AwaitingBroadcast::<T, I>::contains_key(id),
				Error::<T, I>::InvalidBroadcastId
			);

			// Remove the broadcast now it's completed.
			AwaitingBroadcast::<T, I>::remove(id);
			Self::deposit_event(Event::<T, I>::BroadcastComplete(id));

			Ok(().into())
		}

		/// Nodes have witnessed that something went wrong. The transaction may have been rejected outright or may
		/// have stalled on the target chain.
		#[pallet::weight(10_000)]
		pub fn broadcast_failure(
			origin: OriginFor<T>,
			id: BroadcastId,
			failure: BroadcastFailure,
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureWitnessed::ensure_origin(origin)?;

			let failed_attempt =
				AwaitingBroadcast::<T, I>::take(id).ok_or(Error::<T, I>::InvalidBroadcastId)?;

			match failure {
				BroadcastFailure::TransactionRejected => {
					Self::report_and_retry(
						id,
						&failed_attempt.signer.clone(),
						failed_attempt.into(),
					);
				}
				BroadcastFailure::TransactionTimeout => {
					Self::retry(id, failed_attempt.into());
				}
				BroadcastFailure::TransactionFailed => {
					Self::deposit_event(Event::<T, I>::BroadcastFailed(
						id,
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
	fn broadcast_attempt(unsigned_tx: UnsignedTransactionFor<T, I>, attempt: AttemptCount) {
		// Get a new id.
		let id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Select a signer for this broadcast.
		let nominated_signer = T::SignerNomination::nomination_with_seed(id);

		AwaitingSignature::<T, I>::insert(
			id,
			SigningAttempt::<T, I> {
				unsigned_tx: unsigned_tx.clone(),
				nominee: nominated_signer.clone(),
				attempt_count: attempt,
			},
		);

		// Emit the transaction signing request.
		Self::deposit_event(Event::<T, I>::TransactionSigningRequest(
			id,
			nominated_signer,
			unsigned_tx,
		));
	}

	fn report_and_retry(
		id: BroadcastId,
		signer: &T::ValidatorId,
		failed_attempt: FailedAttempt<T, I>,
	) {
		// TODO: set a sensible penalty and centralise. See #569
		const PENALTY: i32 = 0;
		T::OfflineReporter::report(OfflineCondition::ParticipateSigningFailed, PENALTY, signer)
			.unwrap_or_else(|_| {
				// Should never fail unless the validator doesn't exist.
				frame_support::debug::error!("Unable to report unknown validator {:?}", signer);
				0
			});
		Self::retry(id, failed_attempt);
	}

	fn retry(id: BroadcastId, failed_attempt: FailedAttempt<T, I>) {
		let failed_attempt_count = failed_attempt.attempt_count;
		RetryQueue::<T, I>::append(failed_attempt);
		Self::deposit_event(Event::<T, I>::RetryScheduled(id, failed_attempt_count));
	}
}
