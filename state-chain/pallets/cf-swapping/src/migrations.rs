use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;
pub mod on_chain_trading_migration;
pub mod swap_and_swap_request_migration;

pub type PalletMigration<T> = (
	VersionedMigration<
		6,
		7,
		swap_and_swap_request_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		7,
		8,
		swap_and_swap_request_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<8, Pallet<T>>,
);
