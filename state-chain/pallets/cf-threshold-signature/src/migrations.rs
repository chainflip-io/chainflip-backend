use crate::Pallet;
use cf_runtime_upgrade_utilities::{migration_template, VersionedMigration};

mod v4;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, v4::Migration<T, I>, 4, 5>,
	VersionedMigration<Pallet<T, I>, migration_template::Migration<T>, 5, 6>,
);
