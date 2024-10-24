use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod delete_old_epoch_data;
mod rename_blocks_per_epoch;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, delete_old_epoch_data::Migration<T>, 3, 4>,
	VersionedMigration<Pallet<T>, rename_blocks_per_epoch::BlocksPerEpochMigration<T>, 4, 5>,
	PlaceholderMigration<Pallet<T>, 5>,
);
