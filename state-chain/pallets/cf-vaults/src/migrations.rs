use crate::Pallet;
use cf_runtime_upgrade_utilities::{migration_template, VersionedMigration};

mod v3;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, v3::Migration<T, I>, 3, 4>,
	VersionedMigration<Pallet<T, I>, migration_template::Migration<T>, 4, 5>,
);
