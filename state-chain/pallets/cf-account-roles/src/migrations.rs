use cf_runtime_upgrade_utilities::VersionedMigration;

mod remove_swapping_enabled;

// Migration 1->2 is in the runtime/src/lib.rs `VanityNamesMigration`
pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, remove_swapping_enabled::Migration<T>, 0, 1>,);
