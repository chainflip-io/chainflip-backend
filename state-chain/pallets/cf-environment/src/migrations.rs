pub mod v3;
pub mod v4;
pub mod v5;
pub mod v6;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> = (
	VersionedMigration<crate::Pallet<T>, v4::Migration<T>, 3, 4>,
	VersionedMigration<crate::Pallet<T>, v5::Migration<T>, 4, 5>,
	VersionedMigration<crate::Pallet<T>, v6::Migration<T>, 5, 6>,
	VersionedMigration<crate::Pallet<T>, v6::Migration<T>, 6, 7>,
);
