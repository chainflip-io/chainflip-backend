pub(crate) mod reputation_penalty_storage;
pub(crate) mod suspensions_from_online_pallet;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub(crate) type PalletMigration<T> = (
	VersionedMigration<crate::Pallet<T>, reputation_penalty_storage::Migration<T>, 0, 1>,
	VersionedMigration<crate::Pallet<T>, suspensions_from_online_pallet::Migration<T>, 1, 2>,
);
