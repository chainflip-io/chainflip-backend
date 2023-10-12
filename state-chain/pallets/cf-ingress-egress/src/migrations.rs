pub mod ingress_expiry;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> =
	(VersionedMigration<crate::Pallet<T, I>, ingress_expiry::Migration<T, I>, 0, 1>,);
