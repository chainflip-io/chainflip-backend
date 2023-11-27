pub mod add_initiated_at;
pub mod v1;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> =
	(VersionedMigration<crate::Pallet<T, I>, add_initiated_at::Migration<T, I>, 0, 1>,);
