use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;

mod rename_blocks_per_epoch;

pub type PalletMigration<T> =
	(VersionedMigration<Pallet<T>, rename_blocks_per_epoch::BlocksPerEpochMigration<T>, 3, 4>,);
