use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod add_dca_params;
mod remove_max_swap_retry_duration;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_dca_params::Migration<T, I>, 12, 13>,
	VersionedMigration<Pallet<T, I>, remove_max_swap_retry_duration::Migration<T, I>, 13, 14>,
	PlaceholderMigration<Pallet<T, I>, 14>,
);
