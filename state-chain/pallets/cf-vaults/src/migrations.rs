use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;

mod v4;

pub type PalletMigration<T, I> = (VersionedMigration<Pallet<T, I>, v4::Migration<T, I>, 4, 5>,);
