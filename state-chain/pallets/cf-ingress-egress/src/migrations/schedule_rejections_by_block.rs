// Copyright 2026 Chainflip Labs GmbH
// SPDX-License-Identifier: Apache-2.0

use crate::{Config, ScheduledTransactionsForRejection};
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::{traits::One, Saturating},
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use crate::TransactionRejectionDetails;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use frame_system::pallet_prelude::BlockNumberFor;
#[cfg(feature = "try-runtime")]
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

mod old {
	use crate::{Config, Pallet, TransactionRejectionDetails};
	use frame_support::{pallet_prelude::ValueQuery, Twox64Concat};
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_std::vec::Vec;

	#[frame_support::storage_alias]
	pub(super) type ScheduledTransactionsForRejection<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;

	/// Per-block queue added alongside the legacy queue in the 2.2/2.3 cherry-pick (#7055).
	#[frame_support::storage_alias]
	pub(super) type ScheduledTransactionsForRejectionByBlock<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<TransactionRejectionDetails<T, I>>,
		ValueQuery,
	>;
}

pub struct Migration<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let pending = old::ScheduledTransactionsForRejection::<T, I>::take();
		let mut weight = T::DbWeight::get().reads_writes(1, 1);
		if !pending.is_empty() {
			// Legacy refunds are already due. The next block is safe regardless of migration order.
			let process_at = frame_system::Pallet::<T>::block_number().saturating_add(One::one());
			ScheduledTransactionsForRejection::<T, I>::insert(process_at, pending);
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
		}
		for (process_at, scheduled) in
			old::ScheduledTransactionsForRejectionByBlock::<T, I>::drain()
		{
			ScheduledTransactionsForRejection::<T, I>::mutate(process_at, |queue| {
				queue.extend(scheduled)
			});
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));
		}
		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		ensure!(
			ScheduledTransactionsForRejection::<T, I>::iter().next().is_none(),
			"Rejection buckets must be empty before migration"
		);
		let mut expected: BTreeMap<BlockNumberFor<T>, Vec<TransactionRejectionDetails<T, I>>> =
			BTreeMap::new();
		let legacy = old::ScheduledTransactionsForRejection::<T, I>::get();
		if !legacy.is_empty() {
			expected.insert(
				frame_system::Pallet::<T>::block_number().saturating_add(One::one()),
				legacy,
			);
		}
		for (process_at, scheduled) in old::ScheduledTransactionsForRejectionByBlock::<T, I>::iter()
		{
			expected.entry(process_at).or_default().extend(scheduled);
		}
		Ok(expected.into_iter().collect::<Vec<_>>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let expected = <Vec<(BlockNumberFor<T>, Vec<TransactionRejectionDetails<T, I>>)>>::decode(
			&mut &state[..],
		)
		.map_err(|_| "Failed to decode rejection migration state")?;
		ensure!(
			!old::ScheduledTransactionsForRejection::<T, I>::exists(),
			"Legacy rejection queue must be removed"
		);
		ensure!(
			old::ScheduledTransactionsForRejectionByBlock::<T, I>::iter().next().is_none(),
			"Legacy per-block rejection queue must be drained"
		);
		ensure!(
			ScheduledTransactionsForRejection::<T, I>::iter().count() == expected.len(),
			"Unexpected rejection bucket count"
		);
		for (process_at, scheduled) in expected {
			ensure!(
				ScheduledTransactionsForRejection::<T, I>::get(process_at).encode() ==
					scheduled.encode(),
				"Migrated refunds must match the legacy queues"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		migrations::ScheduleRejectionsByBlock as PalletMigration,
		mocks::{new_test_ext, System, Test},
		Pallet, TransactionRejectionDetails, STORAGE_VERSION,
	};
	use cf_chains::{assets::eth::Asset, ForeignChainAddress};
	use codec::Encode;
	use frame_support::{
		instances::{Instance1, Instance2},
		traits::{OnRuntimeUpgrade, StorageVersion},
	};

	#[test]
	fn migration_preserves_pending_refunds_and_runs_once() {
		new_test_ext().execute_with(|| {
			System::set_block_number(10);
			let pending: Vec<_> = [100, 200]
				.into_iter()
				.map(|amount| TransactionRejectionDetails::<Test, Instance1> {
					deposit_address: Some([1; 20].into()),
					refund_address: ForeignChainAddress::Eth([2; 20].into()),
					asset: Asset::Eth,
					amount,
					deposit_details: Default::default(),
					refund_ccm_metadata: None,
				})
				.collect();
			old::ScheduledTransactionsForRejection::<Test, Instance1>::put(&pending);
			StorageVersion::new(31).put::<Pallet<Test, Instance1>>();
			StorageVersion::new(31).put::<Pallet<Test, Instance2>>();

			#[cfg(feature = "try-runtime")]
			PalletMigration::<Test, Instance1>::try_on_runtime_upgrade(true).unwrap();
			#[cfg(not(feature = "try-runtime"))]
			PalletMigration::<Test, Instance1>::on_runtime_upgrade();

			assert_eq!(
				ScheduledTransactionsForRejection::<Test, Instance1>::get(11).encode(),
				pending.encode()
			);
			assert!(!old::ScheduledTransactionsForRejection::<Test, Instance1>::exists());
			assert_eq!(StorageVersion::get::<Pallet<Test, Instance1>>(), STORAGE_VERSION);
			assert_eq!(StorageVersion::get::<Pallet<Test, Instance2>>(), StorageVersion::new(31));
			PalletMigration::<Test, Instance1>::on_runtime_upgrade();
			assert_eq!(
				ScheduledTransactionsForRejection::<Test, Instance1>::get(11).encode(),
				pending.encode()
			);
			assert_eq!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().count(), 1);
		});
	}

	#[test]
	fn migration_drains_the_per_block_queue() {
		new_test_ext().execute_with(|| {
			System::set_block_number(10);
			let details = |amount| TransactionRejectionDetails::<Test, Instance1> {
				deposit_address: Some([1; 20].into()),
				refund_address: ForeignChainAddress::Eth([2; 20].into()),
				asset: Asset::Eth,
				amount,
				deposit_details: Default::default(),
				refund_ccm_metadata: None,
			};
			old::ScheduledTransactionsForRejection::<Test, Instance1>::put(vec![details(100)]);
			// Shares the legacy queue's target block, so the two must be merged.
			old::ScheduledTransactionsForRejectionByBlock::<Test, Instance1>::insert(
				11,
				vec![details(200)],
			);
			old::ScheduledTransactionsForRejectionByBlock::<Test, Instance1>::insert(
				50,
				vec![details(300), details(400)],
			);
			StorageVersion::new(31).put::<Pallet<Test, Instance1>>();

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test, Instance1>::pre_upgrade().unwrap();
			PalletMigration::<Test, Instance1>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			Migration::<Test, Instance1>::post_upgrade(state).unwrap();

			assert_eq!(
				ScheduledTransactionsForRejection::<Test, Instance1>::get(11).encode(),
				vec![details(100), details(200)].encode()
			);
			assert_eq!(
				ScheduledTransactionsForRejection::<Test, Instance1>::get(50).encode(),
				vec![details(300), details(400)].encode()
			);
			assert_eq!(ScheduledTransactionsForRejection::<Test, Instance1>::iter().count(), 2);
			assert!(old::ScheduledTransactionsForRejectionByBlock::<Test, Instance1>::iter()
				.next()
				.is_none());
		});
	}

	#[test]
	fn migration_does_not_create_an_empty_bucket() {
		new_test_ext().execute_with(|| {
			old::ScheduledTransactionsForRejection::<Test, Instance2>::put(Vec::<
				TransactionRejectionDetails<Test, Instance2>,
			>::new());
			StorageVersion::new(31).put::<Pallet<Test, Instance2>>();
			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test, Instance2>::pre_upgrade().unwrap();
			PalletMigration::<Test, Instance2>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			Migration::<Test, Instance2>::post_upgrade(state).unwrap();
			assert!(!old::ScheduledTransactionsForRejection::<Test, Instance2>::exists());
			assert!(ScheduledTransactionsForRejection::<Test, Instance2>::iter().next().is_none());
			assert_eq!(StorageVersion::get::<Pallet<Test, Instance2>>(), STORAGE_VERSION);
		});
	}
}
