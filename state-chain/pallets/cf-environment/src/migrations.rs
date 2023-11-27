use crate::{Config, Pallet};
use frame_support::traits::OnRuntimeUpgrade;
#[cfg(feature = "try-runtime")]
use frame_support::{sp_runtime, sp_runtime::traits::Get};

// Always run this after all others have finished.
pub struct VersionUpdate<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for VersionUpdate<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Pallet::<T>::update_current_release_version();
		frame_support::weights::Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			crate::CurrentReleaseVersion::<T>::get() < <T as Config>::CurrentReleaseVersion::get(),
			"Expected the release version to increase."
		);
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			crate::CurrentReleaseVersion::<T>::get() == <T as Config>::CurrentReleaseVersion::get(),
			"Expected the release version to be updated."
		);
		Ok(())
	}
}

pub type PalletMigration<T> = (VersionUpdate<T>,);
