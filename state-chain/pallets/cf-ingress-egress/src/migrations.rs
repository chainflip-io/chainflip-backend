use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};
mod add_dca_params;
mod remove_deposit_tracker;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, remove_deposit_tracker::Migration<T, I>, 12, 13>,
	VersionedMigration<Pallet<T, I>, add_dca_params::Migration<T, I>, 13, 14>,
	PlaceholderMigration<Pallet<T, I>, 14>,
);
