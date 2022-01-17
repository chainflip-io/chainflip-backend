use super::*;

pub(crate) mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");
		Ok(())
	}

	pub fn migrate<T: Config>() {
		MintInterval::<T>::put(T::BlockNumber::from(100 as u32));
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);
		assert_eq!(T::MintInterval::get(), MintInterval::<T>::get());
		log::info!(
			target: "runtime::cf_emissions",
			"migration: Emissions storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
