use crate::Pallet;

use cf_runtime_upgrade_utilities::{migration_template::Migration, VersionedMigration};

pub type PalletMigration<T> = VersionedMigration<Pallet<T>, Migration<T>, 3, 4>;
