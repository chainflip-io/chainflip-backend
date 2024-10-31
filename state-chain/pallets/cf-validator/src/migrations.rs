use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
mod rename_blocks_per_epoch;

pub type PalletMigration<T> = (
	VersionedMigration<Pallet<T>, rename_blocks_per_epoch::BlocksPerEpochMigration<T>, 4, 5>,
	PlaceholderMigration<Pallet<T>, 5>,
);
