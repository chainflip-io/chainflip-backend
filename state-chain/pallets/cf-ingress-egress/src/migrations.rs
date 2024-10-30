use crate::Pallet;
use cf_runtime_upgrade_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

mod add_owner_to_channel_details;
pub type PalletMigration<T, I> = (
	VersionedMigration<
		15,
		16,
		add_owner_to_channel_details::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<16, Pallet<T, I>>,
);
