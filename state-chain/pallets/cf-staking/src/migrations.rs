use frame_support::traits::Get;

use super::*;

pub(crate) mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(false);
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");
		assert!(T::EpochInfo::blocks_per_epoch() > Zero::zero(), "we should have blocks per epoch");

		log::info!(
			target: "runtime::cf_staking",
			"migration: Staking storage version v1 PRE migration checks successful!"
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		// Current version is genesis, upgrade to version 1
		// Changes are the addition of one storage item: `ClaimExclusionPeriod`
		// We expect this to be half the current epoch and will use the `EpochInfo` for this
		ClaimExclusionPeriod::<T>::put(T::EpochInfo::blocks_per_epoch() / 2u32.into());
		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);
		assert_eq!(
			ClaimExclusionPeriod::<T>::get(),
			T::EpochInfo::blocks_per_epoch() / 2u32.into()
		);

		log::info!(
			target: "runtime::cf_staking",
			"migration: Staking storage version v1 POST migration checks successful!"
		);

		Ok(())
	}
}
