use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod move_asset_balances;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, move_asset_balances::Migration<T>, 1, 2>,
	PlaceholderMigration<Pallet<T>, 2>,
);
