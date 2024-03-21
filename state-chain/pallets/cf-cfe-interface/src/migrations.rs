use cf_runtime_upgrade_utilities::{migration_template, VersionedMigration};

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, migration_template::Migration<T>, 0, 1>,);
