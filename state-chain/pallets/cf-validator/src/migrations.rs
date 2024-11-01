use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

mod delete_old_epoch_data;

pub type PalletMigration<T> = (
	VersionedMigration<
		3,
		4,
		delete_old_epoch_data::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<4, Pallet<T>>,
);
