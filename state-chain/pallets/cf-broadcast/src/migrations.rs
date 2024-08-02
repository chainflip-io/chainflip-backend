use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod move_transaction_fee_deficit;
pub mod v4;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, v4::Migration<T, I>, 3, 4>,
	VersionedMigration<Pallet<T, I>, move_transaction_fee_deficit::Migration<T, I>, 4, 5>,
	// migration 5 to 6 done at the runtime level
	PlaceholderMigration<Pallet<T, I>, 6>,
);
