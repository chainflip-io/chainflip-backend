use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod v4;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, v4::Migration<T, I>, 3, 4>,
	PlaceholderMigration<Pallet<T, I>, 4>,
);
