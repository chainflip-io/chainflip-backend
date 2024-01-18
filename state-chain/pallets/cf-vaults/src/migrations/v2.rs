use crate::*;
use cf_primitives::FLIPPERINOS_PER_FLIP;
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

/// v2 migrating storage item KeygenSlashRate -> KeygenSlashAmount
/// Percent -> FlipBalance
pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {

	use super::*;

	use frame_support::{pallet_prelude::ValueQuery, sp_runtime::Percent};

	#[frame_support::storage_alias]
	pub type KeygenSlashRate<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Percent, ValueQuery>;
}

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// remove the old storage containing Percent
		old::KeygenSlashRate::<T, I>::kill();

		// set the new storage containing the absolute FlipAmount
		KeygenSlashAmount::<T, I>::put(FLIPPERINOS_PER_FLIP);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		// just check that the old storage item existed
		Ok(old::KeygenSlashRate::<T, I>::exists().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if <bool>::decode(&mut &state[..]).map_err(|_| "Failed to decode pre-upgrade state.")? {
			assert!(!old::KeygenSlashRate::<T, I>::exists());
			assert!(KeygenSlashAmount::<T, I>::exists());
			assert!(KeygenSlashAmount::<T, I>::get() == FLIPPERINOS_PER_FLIP)
		}
		Ok(())
	}
}
