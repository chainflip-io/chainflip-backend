use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;
mod separate_swap_state;

pub type PalletMigration<T> =
	VersionedMigration<Pallet<T>, separate_swap_state::Migration<T>, 3, 4>;
