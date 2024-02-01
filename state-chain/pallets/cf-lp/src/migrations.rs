pub mod remove_expiries;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, remove_expiries::Migration<T>, 0, 1>,);
