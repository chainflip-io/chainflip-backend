use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod v1;

pub type PalletMigration<T> =
	(VersionedMigration<Pallet<T>, v1::Migration<T>, 0, 1>, PlaceholderMigration<Pallet<T>, 1>);
