use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod move_network_fees;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, move_network_fees::Migration<T>, 4, 5>,
	PlaceholderMigration<Pallet<T>, 5>,
);

#[cfg(feature = "try-runtime")]
pub mod old {
	use crate::*;
	use frame_support::pallet_prelude::ValueQuery;
	use frame_system::pallet_prelude::BlockNumberFor;

	// Migration 4->5 is in the runtime/src/lib.rs `NetworkFeesMigration`
	#[frame_support::storage_alias]
	pub type FlipBuyInterval<T: Config> = StorageValue<Pallet<T>, BlockNumberFor<T>, ValueQuery>;
	#[frame_support::storage_alias]
	pub type CollectedNetworkFee<T: Config> = StorageValue<Pallet<T>, AssetAmount, ValueQuery>;
}
