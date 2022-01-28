use super::*;

pub(crate) mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");

		log::info!(
			target: "runtime::cf_validator",
			"migration: Validator storage version v1 PRE migration checks successful!"
		);

		Ok(())
	}

	pub fn migrate<T: Config>() -> frame_support::weights::Weight {
		0
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);

		Ok(())
	}
}
