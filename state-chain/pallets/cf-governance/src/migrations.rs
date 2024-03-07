use crate::Pallet;
use cf_runtime_upgrade_utilities::{migration_template, VersionedMigration};

pub mod v2;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, v2::Migration<T>, 1, 2>,
	VersionedMigration<Pallet<T>, migration_template::Migration<T>, 2, 3>,
);
