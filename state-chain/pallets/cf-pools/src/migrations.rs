pub mod v4;

use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> = (VersionedMigration<Pallet<T>, v4::Migration<T>, 3, 4>,);
