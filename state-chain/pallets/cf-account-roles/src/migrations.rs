use crate::Pallet;
use cf_runtime_upgrade_utilities::{NoopRuntimeUpgrade, VersionedMigration};

mod remove_swapping_enabled;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, remove_swapping_enabled::Migration<T>, 0, 1>,
	VersionedMigration<
		Pallet<T>,
		NoopRuntimeUpgrade,
		// Migration 1 -> 2 is in the runtime/src/lib.rs:
		// - VanityNamesMigration
		1,
		2,
	>,
);

pub mod vanity_name_migration {
	pub const APPLY_AT_ACCOUNT_ROLES_STORAGE_VERSION: u16 = 2;
}
