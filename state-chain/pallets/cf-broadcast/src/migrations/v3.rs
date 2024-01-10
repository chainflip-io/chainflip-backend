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

	#[frame_support::storage_alias]
	pub type Timeouts<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BlockNumberFor<T>, Vec<BroadcastAttemptId>>;
}

#[derive(Clone, Encode, Decode)]
pub struct MigrationVerification<T: Chainflip> {
	broadcasts_with_failed_broadcasters:
		Vec<(BroadcastId, BTreeSet<<T as Chainflip>::ValidatorId>)>,
	broadcast_retry_queue: BTreeSet<BroadcastId>,
	timeout_broadcasts: BTreeMap<BlockNumberFor<T>, BTreeSet<BroadcastId>>,
}
pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Awaiting broadcast: take the broadcast attempt with the highest attempt.
		let mut latest_attempt = BTreeMap::new();
		old::AwaitingBroadcast::<T, I>::drain().for_each(
			|(old::BroadcastAttemptId { broadcast_id, attempt_count }, attempt)| {
				if match latest_attempt.get(&broadcast_id) {
					Some((latest_attempt, _attempt)) => attempt_count > *latest_attempt,
					None => true,
				} {
					latest_attempt.insert(broadcast_id, (attempt_count, attempt));
				}
			},
		);

		// Migrate data from old storage into new storage.
		latest_attempt
			.into_iter()
			.for_each(|(broadcast_id, (_attempt_count, attempt))| {
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
				)
			});

		// Failed broadcaster storage: Vec<ValidatorId> -> BTreeSet<ValidatorId> - dedup
		FailedBroadcasters::<T, I>::translate(|_, failed_broadcasters: Vec<T::ValidatorId>| {
			Some(BTreeSet::from_iter(failed_broadcasters))
		});

		// Latest Retry Queue: Dedup and translate to the new struct.
		let mut added_retries = BTreeSet::new();
		let retries = old::BroadcastRetryQueue::<T, I>::take()
			.unwrap_or_default()
			.into_iter()
			.filter_map(|attempt| {
				let broadcast_id = attempt.broadcast_attempt_id.broadcast_id;
				if !added_retries.contains(&broadcast_id) {
					added_retries.insert(broadcast_id);
					Some(BroadcastAttempt::<T, I> {
						broadcast_id: attempt.broadcast_attempt_id.broadcast_id,
						transaction_payload: attempt.transaction_payload,
						threshold_signature_payload: attempt.threshold_signature_payload,
						transaction_out_id: attempt.transaction_out_id,
					})
				} else {
					None
				}
			})
			.collect::<Vec<_>>();
		BroadcastRetryQueue::<T, I>::set(retries);

		// Migrate Timeouts: Map<Block -> Vec<BroadcastAttemptId>> -> Map<Block -> Vec<BroadcastId>>

		Timeouts::<T, I>::translate(|_, failed_broadcasters: Vec<old::BroadcastAttemptId>| {
			Some(
				failed_broadcasters
					.into_iter()
					.map(|attempt| attempt.broadcast_id)
					.collect::<BTreeSet<_>>(),
			)
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let broadcasts_with_failed_broadcasters = old::FailedBroadcasters::<T, I>::iter()
			.map(|(broadcast_id, failed_broadcasters)| {
				(broadcast_id, failed_broadcasters.into_iter().collect())
			})
			.collect::<Vec<_>>();
		let broadcast_retry_queue = old::BroadcastRetryQueue::<T, I>::get()
			.unwrap_or_default()
			.into_iter()
			.map(|attempt| attempt.broadcast_attempt_id.broadcast_id)
			.collect::<BTreeSet<_>>();
		let timeout_broadcasts = old::Timeouts::<T, I>::iter()
			.map(|(block_number, attempts)| {
				(
					block_number,
					attempts.iter().map(|attempt| attempt.broadcast_id).collect::<BTreeSet<_>>(),
				)
			})
			.collect::<BTreeMap<BlockNumberFor<T>, BTreeSet<BroadcastId>>>();
		Ok(MigrationVerification::<T> {
			broadcasts_with_failed_broadcasters,
			broadcast_retry_queue,
			timeout_broadcasts,
		}
		.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		// Decode verification data.
		let MigrationVerification::<T> {
			broadcasts_with_failed_broadcasters,
			broadcast_retry_queue,
			timeout_broadcasts,
		} = MigrationVerification::<T>::decode(&mut &state[..]).unwrap();

		// Ensure all failed broadcasters storage are migrated.
		broadcasts_with_failed_broadcasters.into_iter().for_each(
			|(broadcast_id, failed_broadcasters)| {
				assert_eq!(FailedBroadcasters::<T, I>::get(broadcast_id), failed_broadcasters);
			},
		);

		// Ensure retry queues are migrated.
		let new_retry_queue = BroadcastRetryQueue::<T, I>::get()
			.into_iter()
			.map(|attempt| attempt.broadcast_id)
			.collect::<BTreeSet<_>>();
		assert!(broadcast_retry_queue.difference(&new_retry_queue).next().is_none());

		// Ensure Timeouts data are migrated
		timeout_broadcasts.into_iter().for_each(|(block_number, attempts)|
			// Assert the pre- and post- migrated data is identical.
			assert!(attempts.difference(&Timeouts::<T, I>::get(block_number)).next().is_none()));

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use super::*;
	use crate::mock::*;
	#[cfg(feature = "try-runtime")]
	use frame_support::assert_ok;

	// Perform runtime migration. Run pre- and post- upgrade if "try-runtime"
	fn do_upgrade() {
		#[cfg(feature = "try-runtime")]
		let verification = crate::migrations::v3::Migration::<Test, Instance1>::pre_upgrade().unwrap();
		crate::migrations::v3::Migration::<Test, Instance1>::on_runtime_upgrade();
		#[cfg(feature = "try-runtime")]
		assert_ok!(crate::migrations::v3::Migration::<Test, Instance1>::post_upgrade(verification));
	}

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
			do_upgrade();

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
			do_upgrade();

			// Verify data is correctly migrated into new storage.
			let failed_1 = FailedBroadcasters::<Test, Instance1>::get(1);
			let failed_2 = FailedBroadcasters::<Test, Instance1>::get(2);
			let failed_broadcasters = [1u64, 2u64, 3u64].into_iter().collect();
			assert_eq!(failed_1, failed_broadcasters);
			assert_eq!(failed_2, failed_broadcasters);

			assert_eq!(FailedBroadcasters::<Test, Instance1>::decode_len(1), Some(3));
			assert_eq!(FailedBroadcasters::<Test, Instance1>::decode_len(2), Some(3));
		});
	}

	#[test]
	fn migration_works_v2_to_v3_retry_queue() {
		new_test_ext().execute_with(|| {
			// Insert mock data into old storage
			old::BroadcastRetryQueue::<Test, Instance1>::put(
				[0, 0, 1, 1, 2, 2]
					.into_iter()
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
			do_upgrade();

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

	#[test]
	fn migration_works_v2_to_v3_time_outs() {
		new_test_ext().execute_with(|| {
			// Insert mock data into old storage
			old::Timeouts::<Test, Instance1>::insert(
				100u64,
				vec![
					old::BroadcastAttemptId { broadcast_id: 1, attempt_count: 1 },
					old::BroadcastAttemptId { broadcast_id: 2, attempt_count: 2 },
				],
			);
			old::Timeouts::<Test, Instance1>::insert(
				101u64,
				vec![
					old::BroadcastAttemptId { broadcast_id: 3, attempt_count: 1 },
					old::BroadcastAttemptId { broadcast_id: 3, attempt_count: 2 },
					old::BroadcastAttemptId { broadcast_id: 3, attempt_count: 3 },
					old::BroadcastAttemptId { broadcast_id: 4, attempt_count: 1 },
					old::BroadcastAttemptId { broadcast_id: 4, attempt_count: 2 },
				],
			);

			// Perform runtime migration.
			do_upgrade();

			assert_eq!(Timeouts::<Test, Instance1>::get(100), BTreeSet::from_iter(vec![1, 2]));
			assert_eq!(Timeouts::<Test, Instance1>::get(101), BTreeSet::from_iter(vec![3, 4]));
		});
	}
}
