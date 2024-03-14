use crate::*;

use cf_runtime_upgrade_utilities::{migration_template::Migration, VersionedMigration};

mod v1;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, v1::Migration<T>, 1, 2>,
	VersionedMigration<Pallet<T>, Migration<T>, 2, 3>,
);

#[cfg(feature = "try-runtime")]
pub mod old {
	use super::*;
	use cf_primitives::AssetAmount;
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub type FlipToBurn<T: Config> = StorageValue<Pallet<T>, AssetAmount, ValueQuery>;
}
