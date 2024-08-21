use frame_support::traits::OnRuntimeUpgrade;

use crate::*;

mod old {
	use cf_primitives::BlockNumber;

	use super::*;

	#[frame_support::storage_alias]
	pub type MaxSwapRetryDurationBlocks<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, BlockNumber, ValueQuery>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		// MaxSwapRetryDurationBlocks moved to the swapping pallet, but its not worth migrating,
		// just kill and swapping will use the default.
		old::MaxSwapRetryDurationBlocks::<T, I>::kill();
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(old::MaxSwapRetryDurationBlocks::<T, I>::get(), u32::default());
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock_eth::*;

		new_test_ext().execute_with(|| {
			old::MaxSwapRetryDurationBlocks::<Test, _>::set(69);

			// Perform runtime migration.
			super::Migration::<Test, _>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(vec![]).unwrap();

			// Storage is cleared
			assert_eq!(old::MaxSwapRetryDurationBlocks::<Test, _>::get(), u32::default());
		});
	}
}
