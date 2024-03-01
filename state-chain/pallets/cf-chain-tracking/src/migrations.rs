pub mod set_fee_multiplier;
pub mod v2;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, v2::Migration<T, I>, 1, 2>,
	VersionedMigration<crate::Pallet<T, I>, set_fee_multiplier::Migration<T, I>, 6, 7>,
);
