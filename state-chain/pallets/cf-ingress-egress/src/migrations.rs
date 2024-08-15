use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};
mod remove_deposit_tracker;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, remove_deposit_tracker::Migration<T, I>, 12, 13>,
	PlaceholderMigration<Pallet<T, I>, 13>,
);
