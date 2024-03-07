use crate::Pallet;

use cf_runtime_upgrade_utilities::{migration_template::Migration, VersionedMigration};

pub type PalletMigration<T> = VersionedMigration<Pallet<T>, Migration<T>, 1, 2>;

#[cfg(feature = "try-runtime")]
pub mod old {
	use crate::*;
	use cf_primitives::AssetAmount;
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub type FlipToBurn<T: Config> = StorageValue<Pallet<T>, AssetAmount, ValueQuery>;
}
