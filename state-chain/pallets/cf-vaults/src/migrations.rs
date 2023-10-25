pub mod v2;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (VersionedMigration<crate::Pallet<T>, v2::Migration<T, I>, 1, 2>,);
