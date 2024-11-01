use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

mod initialize_broadcast_timeout_storage;
mod migrate_timeouts;
pub mod remove_aborted_broadcasts;

pub type PalletMigration<T, I> = (
	VersionedMigration<
		6,
		7,
		initialize_broadcast_timeout_storage::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		7,
		8,
		migrate_timeouts::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<8, Pallet<T, I>>,
	// Migration 8->9 is SerializeSolanaBroadcastMigration in runtime lib.
	// Migration 9->10 is SolanaEgressSuccessWitnessMigration in runtime lib.
);
