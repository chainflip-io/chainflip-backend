use crate::Pallet;
pub mod v4;

use cf_runtime_upgrade_utilities::VersionedMigration;
mod lp_pools_state_change;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, v4::Migration<T>, 3, 4>,
	VersionedMigration<Pallet<T>, lp_pools_state_change::Migration<T>, 4, 5>,
);
