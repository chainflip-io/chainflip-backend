use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;
pub mod deposit_channel_details_migration;
pub mod rename_scheduled_tx_for_reject;
pub mod scheduled_egress_ccm_migration;
mod update_rejection_params;

pub type PalletMigration<T, I> = (
	/* ALREADY APPLIED ON BERGHAIN
	VersionedMigration<
		17,
		18,
		update_rejection_params::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	 */
	// APPLY THIS ON BERGHAIN
	/*
	VersionedMigration<
		18,
		19,
		deposit_channel_details_migration::DepositChannelDetailsMigration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		19,
		20,
		scheduled_egress_ccm_migration::ScheduledEgressCcmMigration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		20,
		21,
		rename_scheduled_tx_for_reject::RenameScheduledTxForReject<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	*/
	/* ALREADY APPLIED ON PERSA/SISY
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
	VersionedMigration<
		19,
		20,
		rename_scheduled_tx_for_reject::RenameScheduledTxForReject<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	*/
	// APPLY THIS ON PERSA/SISY
	VersionedMigration<
		20,
		21,
		update_rejection_params::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<21, Pallet<T, I>>,
);
