pub mod migrate_broadcast_attempt_id;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, migrate_broadcast_attempt_id::Migration<T>, 0, 1>,);
