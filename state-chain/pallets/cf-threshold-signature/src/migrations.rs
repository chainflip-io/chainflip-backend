pub mod v1;
pub mod v2;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, v1::Migration<T, I>, 0, 1>,
	VersionedMigration<crate::Pallet<T, I>, v2::Migration<T, I>, 1, 2>,
);
