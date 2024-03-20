pub mod set_fee_multiplier;
pub mod v2;

use cf_runtime_upgrade_utilities::{migration_template, VersionedMigration};

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, set_fee_multiplier::Migration<T, I>, 2, 3>,
	VersionedMigration<crate::Pallet<T, I>, migration_template::Migration<(T, I)>, 3, 4>,
);
