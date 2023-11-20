pub mod v2;
pub mod v3;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, v2::Migration<T, I>, 1, 2>,
	VersionedMigration<crate::Pallet<T, I>, v2::Migration<T, I>, 2, 3>,
);
