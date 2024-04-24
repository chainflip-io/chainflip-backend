use crate::Pallet;
use cf_runtime_upgrade_utilities::{NoopRuntimeUpgrade, VersionedMigration};

pub type PalletMigration<T> = (
	VersionedMigration<
		Pallet<T>,
		NoopRuntimeUpgrade,
		// Migration 3 ->. 4 is in the runtime/src/lib.rs:
		// - ActiveBiddersMigration
		3,
		4,
	>,
);

pub mod active_bidders_migration {
	pub const APPLY_AT_FUNDING_STORAGE_VERSION: u16 = 4;
}

pub mod old {
	use crate::*;
	use frame_support::{pallet_prelude::ValueQuery, Blake2_128Concat};

	#[frame_support::storage_alias]
	pub type ActiveBidder<T: crate::Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		bool,
		ValueQuery,
	>;
}
