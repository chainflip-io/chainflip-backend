use super::*;

pub(crate) mod v1 {
	use super::*;

	#[cfg(feature = "try-runtime")]
	pub(crate) fn pre_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert!(P::on_chain_storage_version() == releases::V0, "Storage version too high.");
		Ok(())
	}

	pub fn migrate<T: Config>() {
		// Migration from the runtime parameter value of version V0
		ReputationPointPenalty::<T>::put(ReputationPenalty { points: 1, blocks: 10u32.into() });
	}

	#[cfg(feature = "try-runtime")]
	pub(crate) fn post_migrate<T: Config, P: GetStorageVersion>() -> Result<(), &'static str> {
		assert_eq!(P::on_chain_storage_version(), releases::V1);
		assert_eq!(
			ReputationPointPenalty::<T>::get(),
			ReputationPenalty { points: 1, blocks: (10 as u32).into() }
		);
		log::info!(
			target: "runtime::cf_reputation",
			"migration: Reputation storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
