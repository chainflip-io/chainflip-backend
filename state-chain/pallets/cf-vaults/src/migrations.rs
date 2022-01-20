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
	match StorageVersion::get::<Pallet<T, I>>() {
		releases::V0 => migrations::v1::pre_migration_checks::<T, I>(),
		v => {
			log::debug!("No pre-migration checks defined for storage version {:?}!", v);
			Ok(())
		},
	}
}

#[cfg(feature = "try-runtime")]
pub(crate) fn post_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	match <Pallet<T, I> as GetStorageVersion>::current_storage_version() {
		releases::V1 => migrations::v1::post_migration_checks::<T, I>(),
		v => {
			log::debug!("No post-migration checks defined for storage version {:?}!", v);
			Ok(())
		},
	}
}
