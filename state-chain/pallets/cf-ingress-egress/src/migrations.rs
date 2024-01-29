pub mod btc_deposit_channels;
pub mod ingress_expiry;
mod set_min_egress;
mod witness_safety_margins;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, ingress_expiry::Migration<T, I>, 0, 1>,
	VersionedMigration<crate::Pallet<T, I>, witness_safety_margins::Migration<T, I>, 1, 2>,
	VersionedMigration<crate::Pallet<T, I>, btc_deposit_channels::Migration<T, I>, 2, 3>,
	VersionedMigration<crate::Pallet<T, I>, set_min_egress::Migration<T, I>, 3, 4>,
);
