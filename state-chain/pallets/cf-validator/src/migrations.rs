use crate::Pallet;
use cf_runtime_upgrade_utilities::{NoopRuntimeUpgrade, VersionedMigration};

mod authorities;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, authorities::Migration<T>, 0, 1>,
	// Migrations 1 -> 3 are in the runtime/src/lib.rs:
	// - VanityNamesMigration
	// - ActiveBiddersMigration
	// This ensures the storage version bumps.
	VersionedMigration<Pallet<T>, NoopRuntimeUpgrade, 1, 2>,
	VersionedMigration<Pallet<T>, NoopRuntimeUpgrade, 2, 3>,
);

pub mod vanity_name_migration {
	pub const APPLY_AT_VALIDATOR_STORAGE_VERSION: u16 = 3;
}

pub mod active_bidders_migration {
	pub const APPLY_AT_VALIDATOR_STORAGE_VERSION: u16 = 3;
}

#[cfg(feature = "try-runtime")]
pub mod old {
	use crate::*;
	use cf_primitives::AccountId;
	use frame_support::pallet_prelude::ValueQuery;

	// Migration 1 -> 2 is in the runtime/src/lib.rs `VanityNamesMigration`
	#[frame_support::storage_alias]
	pub type VanityNames<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<AccountId, Vec<u8>>, ValueQuery>;
}
