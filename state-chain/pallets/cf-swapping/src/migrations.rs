mod add_execute_at;
mod remove_expiries;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> = (
	VersionedMigration<crate::Pallet<T>, remove_expiries::Migration<T>, 0, 1>,
	VersionedMigration<crate::Pallet<T>, add_execute_at::Migration<T>, 1, 2>,
);
