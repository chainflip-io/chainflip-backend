pub mod add_initiated_at;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> =
	(VersionedMigration<crate::Pallet<T, I>, add_initiated_at::Migration<T, I>, 0, 1>,);
