use crate::Pallet;

pub mod on_chain_trading_migration;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

pub type PalletMigration<T> = (
	VersionedMigration<
		7,
		8,
		on_chain_trading_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<8, Pallet<T>>,
);
