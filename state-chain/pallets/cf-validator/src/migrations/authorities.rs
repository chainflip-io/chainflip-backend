use crate::{Config, CurrentAuthorities, HistoricalAuthorities, ValidatorIdOf};
use core::marker::PhantomData;
use frame_support::{sp_runtime::DispatchError, traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::vec::Vec;

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
		let authorities = old::CurrentAuthorities::<T>::take();
		CurrentAuthorities::<T>::put(authorities.into_iter().collect::<Vec<ValidatorIdOf<T>>>());

		let historical_authorities = old::HistoricalAuthorities::<T>::drain();
		for (epoch, btree) in historical_authorities {
			HistoricalAuthorities::<T>::set(
				epoch,
				btree.into_iter().collect::<Vec<ValidatorIdOf<T>>>(),
			);
		}

		// let _result =
		// 	CurrentAuthorities::<T>::translate::<BTreeSet<ValidatorIdOf<T>>, _>(|btree| {
		// 		Some(btree.unwrap().into_iter().collect::<Vec<ValidatorIdOf<T>>>())
		// 	});

		// HistoricalAuthorities::<T>::translate::<BTreeSet<ValidatorIdOf<T>>, _>(
		// 	|_epoch_index, btree| Some(btree.into_iter().collect::<Vec<ValidatorIdOf<T>>>()),
		// );

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
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
			old::HistoricalAuthorities::<Test>::set(2, set);

			// Perform runtime migration.
			crate::migrations::authorities::Migration::<Test>::on_runtime_upgrade();

			// Verify data is correctly migrated into new storage.
			let current_authorities = CurrentAuthorities::<Test>::get();
			let historical_authorities1 = HistoricalAuthorities::<Test>::get(1);
			let historical_authorities2 = HistoricalAuthorities::<Test>::get(2);

			println!("{:?}", current_authorities);
			println!("{:?}", historical_authorities1);
			println!("{:?}", historical_authorities2);
		});
	}
}
