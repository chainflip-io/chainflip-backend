use crate::*;
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;

pub mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type BlocksPerEpoch<T: Config> = StorageValue<Pallet<T>, BlockNumberFor<T>, ValueQuery>;
}

pub struct BlocksPerEpochMigration<T: Config>(sp_std::marker::PhantomData<T>);

// Rename BlocksPerEpoch -> EpochDuration
impl<T: Config> UncheckedOnRuntimeUpgrade for BlocksPerEpochMigration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(old::BlocksPerEpoch::<T>::get().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		EpochDuration::<T>::put(old::BlocksPerEpoch::<T>::take());
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			EpochDuration::<T>::get(),
			BlockNumberFor::<T>::decode(&mut &state[..]).unwrap()
		);
		assert!(!old::BlocksPerEpoch::<T>::exists());
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use crate::mock::Test;

	use self::mock::new_test_ext;

	use super::*;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			old::BlocksPerEpoch::<Test>::put(100);
			assert_ne!(EpochDuration::<Test>::get(), 100);

			#[cfg(feature = "try-runtime")]
			let state: Vec<u8> = BlocksPerEpochMigration::<Test>::pre_upgrade().unwrap();

			BlocksPerEpochMigration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			BlocksPerEpochMigration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(EpochDuration::<Test>::get(), 100);
		});
	}
}
