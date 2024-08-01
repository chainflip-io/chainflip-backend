use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};
mod swapping_redesign;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, swapping_redesign::Migration<T>, 5, 6>,
	PlaceholderMigration<Pallet<T>, 6>,
);
