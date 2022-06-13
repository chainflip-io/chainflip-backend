pub mod v1;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> =
	(VersionedMigration<crate::Pallet<T, I>, v1::Migration<T, I>, 0, 1>,);
