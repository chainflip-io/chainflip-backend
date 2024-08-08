use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod api_call_to_pending;
pub mod move_transaction_fee_deficit;

pub type PalletMigration<T, I> = (
	// migration 3 to 4 done at the runtime level
	VersionedMigration<Pallet<T, I>, move_transaction_fee_deficit::Migration<T, I>, 4, 5>,
	VersionedMigration<Pallet<T, I>, api_call_to_pending::Migration<T, I>, 5, 6>,
	PlaceholderMigration<Pallet<T, I>, 6>,
);
