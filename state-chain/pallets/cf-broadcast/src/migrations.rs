use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T, I> = (
	// Migration 9->10 is SolanaEgressSuccessWitnessMigration in runtime lib.
	PlaceholderMigration<10, Pallet<T, I>>,
);
