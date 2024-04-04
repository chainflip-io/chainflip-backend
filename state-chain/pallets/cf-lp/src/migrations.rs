use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod v2;

pub type PalletMigration<T> =
	(VersionedMigration<Pallet<T>, v2::Migration<T>, 1, 2>, PlaceholderMigration<Pallet<T>, 2>);
