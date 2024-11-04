use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T> = PlaceholderMigration<5, Pallet<T>>;

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
