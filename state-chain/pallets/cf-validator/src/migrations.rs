use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod delete_old_epoch_data;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, delete_old_epoch_data::Migration<T>, 3, 4>,
	PlaceholderMigration<Pallet<T>, 4>,
);
