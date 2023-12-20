mod consolidation_parameters;

use crate::*;
use cf_runtime_upgrade_utilities::VersionedMigration;
use frame_support::traits::OnRuntimeUpgrade;
#[cfg(feature = "try-runtime")]
use frame_support::{dispatch::DispatchError, sp_runtime};

pub struct VersionUpdate<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for VersionUpdate<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(Default::default())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Pallet::<T>::update_current_release_version();
		frame_support::weights::Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			crate::CurrentReleaseVersion::<T>::get() == <T as Config>::CurrentReleaseVersion::get(),
			"Expect storage to be the new version after upgrade."
		);

		Ok(())
	}
}

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, consolidation_parameters::Migration<T>, 6, 7>,);
