use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod initialize_broadcast_timeout_storage;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, initialize_broadcast_timeout_storage::Migration<T, I>, 7, 8>,
	PlaceholderMigration<Pallet<T, I>, 8>,
);
