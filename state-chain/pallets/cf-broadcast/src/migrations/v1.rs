use crate::*;
use frame_support::migration::{remove_storage_prefix, storage_iter, take_storage_value};
use sp_std::marker::PhantomData;

///
/// - Rename FailedBroadcastAttempt to BroadcastAttempt
/// - Migrates various broadcast structs to use BroadcastAttempt as a common inner between them
pub struct Migration<T: Config<I>, I: 'static>(PhantomData<T>, PhantomData<I>);

const PALLET_NAME: &[u8; 19] = b"EthereumBroadcaster";

const BROADCAST_ATTEMPT_ID_COUNTER: &[u8; 25] = b"BroadcastAttemptIdCounter";

// Contain types of old version to decode into
mod v0 {
	use codec::{Decode, Encode};
	use frame_support::RuntimeDebug;

	use crate::{AttemptCount, BroadcastId, Config, SignedTransactionFor, UnsignedTransactionFor};

	#[cfg(feature = "try-runtime")]
	pub type BroadcastAttemptId = u64;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransmissionAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub signer: T::ValidatorId,
		pub signed_tx: SignedTransactionFor<T, I>,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
		pub nominee: T::ValidatorId,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct FailedBroadcastAttempt<T: Config<I>, I: 'static> {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
	}
}

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	// all transaction signing attempts
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// get each awaiting transaction signature
		let signing_attempts_iter = storage_iter::<v0::TransactionSigningAttempt<T, I>>(
			PALLET_NAME,
			b"AwaitingTransactionSignature",
		);

		let mut num_writes = 0;
		let mut num_reads = 0;
		signing_attempts_iter.drain().into_iter().for_each(|(_, old_signing_attempt)| {
			let broadcast_attempt_id = BroadcastAttemptId {
				broadcast_id: old_signing_attempt.broadcast_id,
				attempt_count: old_signing_attempt.attempt_count,
			};
			let tx_attempt = TransactionSigningAttempt::<T, I> {
				broadcast_attempt: BroadcastAttempt {
					broadcast_attempt_id,
					unsigned_tx: old_signing_attempt.unsigned_tx,
				},
				nominee: old_signing_attempt.nominee,
			};
			BroadcastIdToAttemptNumbers::<T, I>::append(
				broadcast_attempt_id.broadcast_id,
				broadcast_attempt_id.attempt_count,
			);
			AwaitingTransactionSignature::insert(broadcast_attempt_id, tx_attempt);
			num_reads += 1;
			num_writes += 2;
		});

		let transmission_attempts_iter =
			storage_iter::<v0::TransmissionAttempt<T, I>>(PALLET_NAME, b"AwaitingTransmission");

		transmission_attempts_iter
			.drain()
			.into_iter()
			.for_each(|(_, old_transmission_attempt)| {
				let broadcast_attempt_id = BroadcastAttemptId {
					broadcast_id: old_transmission_attempt.broadcast_id,
					attempt_count: old_transmission_attempt.attempt_count,
				};
				let trans_attempt = TransmissionAttempt::<T, I> {
					broadcast_attempt: BroadcastAttempt {
						broadcast_attempt_id,
						unsigned_tx: old_transmission_attempt.unsigned_tx,
					},
					signer: old_transmission_attempt.signer,
					signed_tx: old_transmission_attempt.signed_tx,
				};
				BroadcastIdToAttemptNumbers::<T, I>::append(
					broadcast_attempt_id.broadcast_id,
					broadcast_attempt_id.attempt_count,
				);
				AwaitingTransmission::insert(broadcast_attempt_id, trans_attempt);
				num_reads += 1;
				num_writes += 2;
			});

		// remove storage prefix

		if let Some(old_retries_vec) = take_storage_value::<Vec<v0::FailedBroadcastAttempt<T, I>>>(
			PALLET_NAME,
			b"BroadcastRetryQueue",
			b"",
		) {
			let queue = old_retries_vec
				.into_iter()
				.map(|failed| BroadcastAttempt::<T, I> {
					broadcast_attempt_id: BroadcastAttemptId {
						broadcast_id: failed.broadcast_id,
						attempt_count: failed.attempt_count,
					},
					unsigned_tx: failed.unsigned_tx,
				})
				.collect::<Vec<_>>();
			num_reads += 1;
			num_writes += 1;
			BroadcastRetryQueue::put(queue);
		}

		// No longer required, we can just use BroadcastId, or aggregate
		// BroadcastAttemptId(BroadcastId, AttemptCount)
		remove_storage_prefix(PALLET_NAME, BROADCAST_ATTEMPT_ID_COUNTER, b"");
		num_writes += 1;
		T::DbWeight::get().reads_writes(num_reads, num_writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use frame_support::migration::get_storage_value;
		let broadcast_attempt_id_counter = get_storage_value::<v0::BroadcastAttemptId>(
			PALLET_NAME,
			BROADCAST_ATTEMPT_ID_COUNTER,
			b"",
		);
		assert!(broadcast_attempt_id_counter.is_some(), "No BroadcastAttemptIdCounter");
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::migration::get_storage_value;
		let broadcast_attempt_id_counter = get_storage_value::<v0::BroadcastAttemptId>(
			PALLET_NAME,
			BROADCAST_ATTEMPT_ID_COUNTER,
			b"",
		);
		// it should not exist

		assert!(broadcast_attempt_id_counter.is_none(), "BroadcastAttemptIdCounter still exists");
		Ok(())
	}
}
