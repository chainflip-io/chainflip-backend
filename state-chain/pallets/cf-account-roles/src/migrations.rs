use cf_runtime_upgrade_utilities::VersionedMigration;

mod remove_swapping_enabled;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, remove_swapping_enabled::Migration<T>, 1, 2>,);
