pub mod v4;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> =
	(VersionedMigration<crate::Pallet<T, I>, v4::Migration<T, I>, 3, 4>,);
