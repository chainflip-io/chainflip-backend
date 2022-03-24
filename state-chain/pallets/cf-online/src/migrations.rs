pub mod remove_liveness;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub(crate) type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, remove_liveness::Migration<T>, 0, 1>,);
