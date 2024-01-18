mod consolidation_parameters;

use crate::*;
use cf_runtime_upgrade_utilities::VersionedMigration;
use frame_support::traits::OnRuntimeUpgrade;
#[cfg(feature = "try-runtime")]
use frame_support::{pallet_prelude::DispatchError, sp_runtime};

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

#[cfg(test)]
mod tests {
	use super::*;

	use crate::mock::{new_test_ext, Test};

	#[test]
	fn version_updates() {
		new_test_ext().execute_with(|| {
			let config_version = <Test as Config>::CurrentReleaseVersion::get();

			let old_version = SemVer { major: 1, minor: 0, patch: 0 };
			assert_ne!(old_version, config_version);
			CurrentReleaseVersion::<Test>::put(old_version);

			VersionUpdate::<Test>::on_runtime_upgrade();

			assert_eq!(CurrentReleaseVersion::<Test>::get(), config_version);
		});
	}
}
