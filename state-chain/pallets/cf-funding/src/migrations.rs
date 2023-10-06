pub mod v1;
pub mod v2;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> = (VersionedMigration<crate::Pallet<T>, v1::Migration<T>, 0, 1>,);
