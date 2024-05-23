use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;

pub mod add_boost_pools;
pub mod multiple_brokers;
pub mod remove_prewitnessed_deposits;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_boost_pools::Migration<T, I>, 6, 7>,
	VersionedMigration<Pallet<T, I>, multiple_brokers::Migration<T, I>, 7, 8>,
	VersionedMigration<Pallet<T, I>, remove_prewitnessed_deposits::Migration<T, I>, 8, 9>,
);
