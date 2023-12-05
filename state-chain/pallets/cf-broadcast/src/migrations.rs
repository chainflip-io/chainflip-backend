pub mod v2;

use cf_runtime_upgrade_utilities::VersionedMigration;

use crate::Pallet;

pub type PalletMigration<T, I> = (VersionedMigration<Pallet<T, I>, v2::Migration<T, I>, 1, 2>,);
