pub mod duration_migration;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, duration_migration::Migration<T>, 0, 1>,);
