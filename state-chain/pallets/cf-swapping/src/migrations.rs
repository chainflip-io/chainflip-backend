use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;
pub mod swap_request_migration;

pub type PalletMigration<T> = (
	VersionedMigration<
		6,
		7,
		swap_request_migration::SwapRequestMigration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<17, Pallet<T>>,
);
