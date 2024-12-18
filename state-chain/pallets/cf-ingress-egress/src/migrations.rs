use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;
pub mod deposit_channel_details_migration;
pub mod scheduled_egress_ccm_migration;

pub type PalletMigration<T, I> = (
	VersionedMigration<
		17,
		18,
		deposit_channel_details_migration::DepositChannelDetailsMigration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		18,
		19,
		scheduled_egress_ccm_migration::ScheduledEgressCcmMigration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<19, Pallet<T, I>>,
);
