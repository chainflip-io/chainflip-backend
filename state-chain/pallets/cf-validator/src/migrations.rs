use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

mod rename_blocks_per_epoch;

pub type PalletMigration<T> = (
	VersionedMigration<
		4,
		5,
		rename_blocks_per_epoch::BlocksPerEpochMigration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<5, Pallet<T>>,
);
