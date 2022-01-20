use super::*;

pub fn migrate_storage<T: Config<I>, I: 'static>() -> frame_support::weights::Weight {
	releases::V1.put::<Pallet<T, I>>();

	0
}

#[cfg(feature = "try-runtime")]
pub fn pre_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	assert_eq!(StorageVersion::get::<Pallet<T, I>>(), releases::V0);

	Ok(())
}

#[cfg(feature = "try-runtime")]
pub fn post_migration_checks<T: Config<I>, I: 'static>() -> Result<(), &'static str> {
	assert_eq!(StorageVersion::get::<Pallet<T, I>>(), releases::V1);

	Ok(())
}
