use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;
mod channel_action_ccm;

pub type PalletMigration<T, I> = (
	VersionedMigration<
		16,
		17,
		channel_action_ccm::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<17, Pallet<T, I>>,
);
