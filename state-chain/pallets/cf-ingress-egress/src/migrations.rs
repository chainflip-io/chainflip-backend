use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod add_boost_pools;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_boost_pools::Migration<T, I>, 6, 7>,
	PlaceholderMigration<Pallet<T, I>, 7>,
);
