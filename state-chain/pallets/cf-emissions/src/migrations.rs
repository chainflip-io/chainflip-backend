use super::*;
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::GetStorageVersion;

pub(crate) mod v1 {
	use super::*;

	// The value for the MintInterval
	// runtime constant in pallet version V0
	const MINT_INTERVAL_V0: u32 = 100;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");
		Ok(())
	}

	pub fn migrate<T: Config>() {
		MintInterval::<T>::put(T::BlockNumber::from(MINT_INTERVAL_V0));
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);
		assert_eq!(T::BlockNumber::from(100 as u32), MintInterval::<T>::get());
		log::info!(
			target: "runtime::cf_emissions",
			"migration: Emissions storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
