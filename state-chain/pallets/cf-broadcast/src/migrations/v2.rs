use crate::*;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_std::prelude::Vec;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

mod old {
	use super::*;

	/// A unique id for each broadcast attempt
	#[derive(
		Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default, Copy,
	)]
	pub struct BroadcastAttemptId {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct BroadcastAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt_id: BroadcastAttemptId,
		pub transaction_payload: TransactionFor<T, I>,
		pub threshold_signature_payload: PayloadFor<T, I>,
		pub transaction_out_id: TransactionOutIdFor<T, I>,
	}

	#[frame_support::storage_alias]
	pub type RequestCallbacks<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;

	#[frame_support::storage_alias]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastAttemptId, TransactionSigningAttempt<T, I>>;

	#[frame_support::storage_alias]
	pub type FailedBroadcasters<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastId, Vec<<T as Chainflip>::ValidatorId>>;

	#[frame_support::storage_alias]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<BroadcastAttempt<T, I>>>;
}

#[derive(Clone, Encode, Decode)]
pub struct MigrationVerification {
	broadcasts_with_failed_broadcasters: Vec<BroadcastId>,
	broadcast_retry_queue: Vec<BroadcastId>,
}
pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use frame_support::storage::StoragePrefixedMap;

		// Renaming of storage: RequestCallbacks -> RequestSuccessCallbacks
		frame_support::migration::move_prefix(
			old::RequestCallbacks::<T, I>::storage_prefix(),
			RequestSuccessCallbacks::<T, I>::storage_prefix(),
		);

		// Adding Awaiting Broadcasts -> PendingBroadcasts
		let pending_broadcasts = old::AwaitingBroadcast::<T, I>::iter_keys()
			.map(|old::BroadcastAttemptId { broadcast_id, .. }| broadcast_id)
			.collect::<BTreeSet<_>>();
		PendingBroadcasts::<T, I>::put(pending_broadcasts);

		// Awaiting broadcast: take the broadcast attempt with the highest attempt.
		let mut latest_attempt = BTreeMap::new();
		old::AwaitingBroadcast::<T, I>::iter_keys().for_each(
			|old::BroadcastAttemptId { broadcast_id, attempt_count }| match latest_attempt
				.get(&broadcast_id)
			{
				Some(attempt) =>
					if attempt_count > *attempt {
						latest_attempt.insert(broadcast_id, attempt_count);
					},
				None => {
					latest_attempt.insert(broadcast_id, attempt_count);
				},
			},
		);

		// Migrate data from old storage into new storage.
		latest_attempt.into_iter().for_each(|(broadcast_id, attempt_count)| {
			if let Some(attempt) = old::AwaitingBroadcast::<T, I>::get(old::BroadcastAttemptId {
				broadcast_id,
				attempt_count,
			}) {
				AwaitingBroadcast::<T, I>::insert(
					broadcast_id,
					TransactionSigningAttempt {
						broadcast_attempt: BroadcastAttempt {
							broadcast_id,
							transaction_payload: attempt.broadcast_attempt.transaction_payload,
							threshold_signature_payload: attempt
								.broadcast_attempt
								.threshold_signature_payload,
							transaction_out_id: attempt.broadcast_attempt.transaction_out_id,
						},
						nominee: attempt.nominee,
					},
				);
			}
		});

		// Failed broadcaster storage: Vec<ValidatorId> -> BTreeSet<ValidatorId>
		old::FailedBroadcasters::<T, I>::drain().for_each(|(broadcast_id, failed_broadcasters)| {
			FailedBroadcasters::<T, I>::insert(
				broadcast_id,
				BTreeSet::from_iter(failed_broadcasters),
			)
		});

		let retry_queue = old::BroadcastRetryQueue::<T, I>::take()
			.unwrap_or_default()
			.into_iter()
			.map(|attempt| BroadcastAttempt::<T, I> {
				broadcast_id: attempt.broadcast_attempt_id.broadcast_id,
				transaction_payload: attempt.transaction_payload,
				threshold_signature_payload: attempt.threshold_signature_payload,
				transaction_out_id: attempt.transaction_out_id,
			})
			.collect::<Vec<_>>();
		BroadcastRetryQueue::<T, I>::put(retry_queue);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let broadcasts_with_failed_broadcasters =
			old::FailedBroadcasters::<T, I>::iter_keys().collect::<Vec<_>>();
		let broadcast_retry_queue = old::BroadcastRetryQueue::<T, I>::get()
			.unwrap_or_default()
			.into_iter()
			.map(|attempt| attempt.broadcast_attempt_id.broadcast_id)
			.collect::<Vec<_>>();
		Ok(MigrationVerification { broadcasts_with_failed_broadcasters, broadcast_retry_queue }
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pending_broadcasts = PendingBroadcasts::<T, I>::get();
		for id in AwaitingBroadcast::<T, I>::iter_keys() {
			assert!(pending_broadcasts.contains(&id));
		}

		let MigrationVerification { broadcasts_with_failed_broadcasters, broadcast_retry_queue } =
			<MigrationVerification>::decode(&mut &state[..]).unwrap();

		broadcasts_with_failed_broadcasters.into_iter().for_each(|broadcast_id| {
			assert!(FailedBroadcasters::<T, I>::contains_key(broadcast_id))
		});

		let new_retry_queue = BroadcastRetryQueue::<T, I>::get()
			.into_iter()
			.map(|attempt| attempt.broadcast_id)
			.collect::<Vec<_>>();

		broadcast_retry_queue
			.into_iter()
			.for_each(|broadcast_id| assert!(new_retry_queue.contains(&broadcast_id)));

		Ok(())
	}
}
