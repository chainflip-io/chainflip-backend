pub mod v3;
pub mod v4;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> = (VersionedMigration<crate::Pallet<T>, v4::Migration<T>, 3, 4>,);
