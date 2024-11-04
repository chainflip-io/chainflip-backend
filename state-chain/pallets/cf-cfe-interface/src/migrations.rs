use cf_runtime_utilities::PlaceholderMigration;

use crate::{Config, Pallet};
use frame_support::traits::OnRuntimeUpgrade;
#[cfg(feature = "try-runtime")]
use frame_support::{pallet_prelude::DispatchError, sp_runtime};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub type PalletMigration<T> = PlaceholderMigration<0, Pallet<T>>;

// This migration should only be run at the start of all migrations, in case another migration
// needs to trigger an event like a Broadcast for example
pub struct ClearEvents<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for ClearEvents<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(Default::default())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		crate::CfeEvents::<T>::kill();
		frame_support::weights::Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			crate::CfeEvents::<T>::get().is_empty(),
			"CfeEvents is not empty after upgrade."
		);

		Ok(())
	}
}
