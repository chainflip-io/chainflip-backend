use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;
pub mod deposit_channel_details_migration;

pub type PalletMigration<T, I> = (
	VersionedMigration<
		16,
		17,
		deposit_channel_details_migration::DepositChannelDetailsMigration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<17, Pallet<T, I>>,
);
