mod schedule_swaps;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, schedule_swaps::Migration<T>, 1, 2>,);
