use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod add_dca_params;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_dca_params::Migration<T, I>, 12, 13>,
	PlaceholderMigration<Pallet<T, I>, 13>,
);
