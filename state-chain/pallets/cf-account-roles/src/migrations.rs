use crate::Pallet;
use cf_runtime_upgrade_utilities::{NoopRuntimeUpgrade, VersionedMigration};

mod remove_swapping_enabled;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, remove_swapping_enabled::Migration<T>, 0, 1>,
	// Migration 1 -> 2 is in the runtime/src/lib.rs `VanityNamesMigration`
	// This ensures the storage version bump.
	VersionedMigration<
		Pallet<T>,
		NoopRuntimeUpgrade,
		{ vanity_name_migration::APPLY_AT_ACCOUNT_ROLES_STORAGE_VERSION - 1 },
		{ vanity_name_migration::APPLY_AT_ACCOUNT_ROLES_STORAGE_VERSION },
	>,
);

pub mod vanity_name_migration {
	pub const APPLY_AT_ACCOUNT_ROLES_STORAGE_VERSION: u16 = 2;
}
