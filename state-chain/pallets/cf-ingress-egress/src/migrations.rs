pub mod ingress_expiry;
mod witness_safety_margins;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, ingress_expiry::Migration<T, I>, 0, 1>,
	VersionedMigration<crate::Pallet<T, I>, witness_safety_margins::Migration<T, I>, 1, 2>,
);
