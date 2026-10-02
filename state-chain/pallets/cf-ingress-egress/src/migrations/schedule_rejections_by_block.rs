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
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use frame_system::pallet_prelude::BlockNumberFor;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet, TransactionRejectionDetails};
	use frame_support::pallet_prelude::ValueQuery;
	use sp_std::vec::Vec;

	#[frame_support::storage_alias]
	pub(super) type ScheduledTransactionsForRejection<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;
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
		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		ensure!(
			ScheduledTransactionsForRejection::<T, I>::iter().next().is_none(),
			"Rejection buckets must be empty before migration"
		);
		Ok((
			frame_system::Pallet::<T>::block_number().saturating_add(One::one()),
			old::ScheduledTransactionsForRejection::<T, I>::get(),
		)
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (process_at, pending) = <(
			BlockNumberFor<T>,
			Vec<crate::TransactionRejectionDetails<T, I>>,
		)>::decode(&mut &state[..])
		.map_err(|_| "Failed to decode rejection migration state")?;
		ensure!(
			!old::ScheduledTransactionsForRejection::<T, I>::exists(),
			"Legacy rejection queue must be removed"
		);
		ensure!(
			ScheduledTransactionsForRejection::<T, I>::iter().count() ==
				usize::from(!pending.is_empty()),
			"Unexpected rejection bucket count"
		);
		ensure!(
			ScheduledTransactionsForRejection::<T, I>::get(process_at).encode() == pending.encode(),
			"Migrated refunds must match the legacy queue"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		migrations::PalletMigration,
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
			let state = Migration::<Test, Instance1>::pre_upgrade().unwrap();
			PalletMigration::<Test, Instance1>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			Migration::<Test, Instance1>::post_upgrade(state).unwrap();

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
