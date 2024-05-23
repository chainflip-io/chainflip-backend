use crate::{Config, CurrentAuthorities, HistoricalAuthorities, ValidatorIdOf};
use core::marker::PhantomData;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::vec::Vec;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use crate::{Config, Pallet, *};

	use cf_primitives::EpochIndex;
	use frame_support::{pallet_prelude::ValueQuery, Twox64Concat};

	#[frame_support::storage_alias(pallet_name)]
	pub type CurrentAuthorities<T: Config> =
		StorageValue<Pallet<T>, BTreeSet<ValidatorIdOf<T>>, ValueQuery>;

	#[frame_support::storage_alias(pallet_name)]
	pub type HistoricalAuthorities<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, EpochIndex, BTreeSet<ValidatorIdOf<T>>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("⏫ Applying Authorities migration");
		let authorities = old::CurrentAuthorities::<T>::take();
		CurrentAuthorities::<T>::put(authorities.into_iter().collect::<Vec<ValidatorIdOf<T>>>());

		let historical_authorities = old::HistoricalAuthorities::<T>::drain();
		for (epoch, btree) in historical_authorities {
			HistoricalAuthorities::<T>::set(
				epoch,
				btree.into_iter().collect::<Vec<ValidatorIdOf<T>>>(),
			);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let authorities = old::CurrentAuthorities::<T>::get();
		Ok(authorities.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use frame_support::ensure;

		let previous = <Vec<ValidatorIdOf<T>>>::decode(&mut &state[..]).unwrap();
		let current = CurrentAuthorities::<T>::get();

		ensure!(previous == current, "Authorities are not the same!");
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use crate::mock::{new_test_ext, Test};

	use super::*;
	use sp_std::collections::btree_set::BTreeSet;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			let mut set = BTreeSet::new();
			set.insert(100);
			set.insert(200);
			old::CurrentAuthorities::<Test>::put(set.clone());
			old::HistoricalAuthorities::<Test>::set(1, set.clone());
			old::HistoricalAuthorities::<Test>::set(2, set.clone());

			// Perform runtime migration.
			crate::migrations::authorities::Migration::<Test>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			let current_authorities = CurrentAuthorities::<Test>::get();
			let historical_authorities1 = HistoricalAuthorities::<Test>::get(1);
			let historical_authorities2 = HistoricalAuthorities::<Test>::get(2);

			assert_eq!(set.clone().into_iter().collect::<Vec<u64>>(), current_authorities);
			assert_eq!(set.clone().into_iter().collect::<Vec<u64>>(), historical_authorities1);
			assert_eq!(set.into_iter().collect::<Vec<u64>>(), historical_authorities2);
		});
	}
}
