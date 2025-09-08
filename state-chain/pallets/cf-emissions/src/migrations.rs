pub mod remove_backups;

use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

pub type PalletMigration<T> = (
	VersionedMigration<
		0,
		1,
		remove_backups::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<1, Pallet<T>>,
);
