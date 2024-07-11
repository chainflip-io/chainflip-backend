use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod move_transaction_fee_deficit;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, move_transaction_fee_deficit::Migration<T, I>, 3, 4>,
	PlaceholderMigration<Pallet<T, I>, 4>,
);
