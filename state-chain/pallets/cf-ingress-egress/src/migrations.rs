use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;

pub mod add_boost_pools;
pub mod multiple_brokers;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_boost_pools::Migration<T, I>, 6, 7>,
	VersionedMigration<Pallet<T, I>, multiple_brokers::Migration<T, I>, 7, 8>,
);
