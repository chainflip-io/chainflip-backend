use cf_runtime_upgrade_utilities::VersionedMigration;

mod authorities;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, authorities::Migration<T>, 1, 2>,);

#[cfg(feature = "try-runtime")]
pub mod old {
	use crate::*;
	use cf_primitives::AccountId;
	use frame_support::pallet_prelude::ValueQuery;

	// Migration 0->1 is in the runtime/src/lib.rs `VanityNamesMigration`
	#[frame_support::storage_alias]
	pub type VanityNames<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<AccountId, Vec<u8>>, ValueQuery>;
}
