use crate::*;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_std::prelude::Vec;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

pub(crate) mod old {
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

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt: BroadcastAttempt<T, I>,
		pub nominee: T::ValidatorId,
	}

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

#[cfg(test)]
mod migration_tests {
	use super::*;
	use crate::mock::*;

	#[test]
	fn migration_works_v2_to_v3_awaiting_broadcast() {
		new_test_ext().execute_with(|| {
			// Insert mock data into old storage
			for broadcast_id in 1..10 {
				for attempt_id in 0..=broadcast_id {
					let broadcast_attempt_id =
						old::BroadcastAttemptId { broadcast_id, attempt_count: attempt_id };
					let singing_attempt = old::TransactionSigningAttempt {
						broadcast_attempt: old::BroadcastAttempt {
							broadcast_attempt_id,
							transaction_payload: Default::default(),
							threshold_signature_payload: Default::default(),
							transaction_out_id: Default::default(),
						},
						nominee: attempt_id as u64,
					};
					old::AwaitingBroadcast::<Test, Instance1>::insert(
						broadcast_attempt_id,
						singing_attempt,
					);
				}
			}

			// Perform runtime migration.
			crate::migrations::v3::Migration::<Test, Instance1>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			// Only the attempt with the highest attempt count is migrated.
			for broadcast_id in 1..10 {
				assert_eq!(
					AwaitingBroadcast::<Test, Instance1>::get(broadcast_id),
					Some(TransactionSigningAttempt {
						broadcast_attempt: BroadcastAttempt {
							broadcast_id,
							transaction_payload: Default::default(),
							threshold_signature_payload: Default::default(),
							transaction_out_id: Default::default(),
						},
						nominee: broadcast_id as u64,
					})
				);
			}
		});
	}

	#[test]
	fn migration_works_v2_to_v3_failed_broadcasters() {
		new_test_ext().execute_with(|| {
			// Insert mock data into old storage
			old::FailedBroadcasters::<Test, Instance1>::insert(
				1,
				vec![1u64, 1u64, 2u64, 2u64, 3u64, 3u64, 3u64, 3u64],
			);
			old::FailedBroadcasters::<Test, Instance1>::insert(2, vec![1u64, 2u64, 3u64]);

			// Perform runtime migration.
			crate::migrations::v3::Migration::<Test, Instance1>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			let failed_1 = FailedBroadcasters::<Test, Instance1>::get(1);
			let failed_2 = FailedBroadcasters::<Test, Instance1>::get(2);
			assert!(failed_1.contains(&1u64));
			assert!(failed_1.contains(&2u64));
			assert!(failed_1.contains(&3u64));

			assert!(failed_2.contains(&1u64));
			assert!(failed_2.contains(&2u64));
			assert!(failed_2.contains(&3u64));
		});
	}

	#[test]
	fn migration_works_v2_to_v3_retry_queue() {
		new_test_ext().execute_with(|| {
			// Insert mock data into old storage
			old::BroadcastRetryQueue::<Test, Instance1>::put(
				(0..3)
					.map(|broadcast_id| old::BroadcastAttempt {
						broadcast_attempt_id: old::BroadcastAttemptId {
							broadcast_id,
							attempt_count: broadcast_id,
						},
						transaction_payload: Default::default(),
						threshold_signature_payload: Default::default(),
						transaction_out_id: Default::default(),
					})
					.collect::<Vec<_>>(),
			);

			// Perform runtime migration.
			crate::migrations::v3::Migration::<Test, Instance1>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			assert_eq!(
				BroadcastRetryQueue::<Test, Instance1>::get(),
				(0..3)
					.map(|broadcast_id| BroadcastAttempt {
						broadcast_id,
						transaction_payload: Default::default(),
						threshold_signature_payload: Default::default(),
						transaction_out_id: Default::default(),
					})
					.collect::<Vec<_>>()
			);
		});
	}
}
