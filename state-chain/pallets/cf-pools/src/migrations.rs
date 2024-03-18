use crate::*;

use cf_runtime_upgrade_utilities::VersionedMigration;

mod lp_pools_state_change;

pub type PalletMigration<T> =
	(VersionedMigration<Pallet<T>, lp_pools_state_change::Migration<T>, 3, 4>,);

#[cfg(feature = "try-runtime")]
pub mod old {
	use super::*;
	use cf_primitives::AssetAmount;
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub type FlipToBurn<T: Config> = StorageValue<Pallet<T>, AssetAmount, ValueQuery>;
}
