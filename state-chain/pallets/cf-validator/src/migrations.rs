use super::*;
use frame_support::traits::{GetStorageVersion, OnRuntimeUpgrade};

pub struct PalletMigration<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for PalletMigration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <Pallet<T> as GetStorageVersion>::on_chain_storage_version() == 0 {
			MaxAuthoritySetContractionPercentage::<T>::put(DEFAULT_MAX_AUTHORITY_SET_CONTRACTION);
		}
		frame_support::weights::Weight::zero()
	}
}
