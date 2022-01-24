use super::*;
use frame_support::traits::StorageVersion;

mod v1;

pub(crate) fn migrate_storage<T: Config<I>, I: 'static>() -> frame_support::weights::Weight {
	match StorageVersion::get::<Pallet<T, I>>() {
		releases::V0 => migrations::v1::migrate_storage::<T, I>(),
		releases::V1 => 0,
		v => {
			log::warn!("No storage upgrade defined for storage version {:?}!", v);
			0
		},
	}
}

#[cfg(feature = "try-runtime")]
pub(crate) fn pre_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	log::info!("🏯 Running pre-migration checks for vaults pallet.");
	let result = match StorageVersion::get::<Pallet<T, I>>() {
		releases::V0 => migrations::v1::pre_migration_checks::<T, I>(),
		v => {
			log::debug!("No pre-migration checks defined for storage version {:?}!", v);
			Ok(())
		},
	};
	log::info!("✅ Finished Vault pre-migration checks.");
	result
}

#[cfg(feature = "try-runtime")]
pub(crate) fn post_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	log::info!("🏯 Running post-migration checks for vaults pallet.");
	let result = match <Pallet<T, I> as GetStorageVersion>::current_storage_version() {
		releases::V1 => migrations::v1::post_migration_checks::<T, I>(),
		v => {
			log::debug!("No post-migration checks defined for storage version {:?}!", v);
			Ok(())
		},
	};
	log::info!("✅ Finished Vault post-migration checks.");
	result
}
