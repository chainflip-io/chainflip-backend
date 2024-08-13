use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};
mod broker_fees_storage_hasher;
mod swapping_redesign;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, swapping_redesign::Migration<T>, 5, 6>,
	VersionedMigration<Pallet<T>, broker_fees_storage_hasher::Migration<T>, 6, 7>,
	PlaceholderMigration<Pallet<T>, 7>,
);
