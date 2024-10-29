use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod initialize_broadcast_timeout_storage;
mod migrate_timeouts;
pub mod remove_aborted_broadcasts;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, initialize_broadcast_timeout_storage::Migration<T, I>, 6, 7>,
	VersionedMigration<Pallet<T, I>, migrate_timeouts::Migration<T, I>, 7, 8>,
	// Migration 8->9 is SerializeSolanaBroadcastMigration in runtime lib.
	// Migration 9->10 is SolanaEgressSuccessWitnessMigration in runtime lib.
	PlaceholderMigration<Pallet<T, I>, 10>,
);
