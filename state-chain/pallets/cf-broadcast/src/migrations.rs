pub mod v3;

use cf_runtime_upgrade_utilities::VersionedMigration;

use crate::Pallet;

pub type PalletMigration<T, I> = (VersionedMigration<Pallet<T, I>, v3::Migration<T, I>, 2, 3>,);
