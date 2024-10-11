use crate::Pallet;
use cf_runtime_upgrade_utilities::PlaceholderMigration;

pub type PalletMigration<T, I> = (
	// Migration 9->10 is SolanaEgressSuccessWitnessMigration in runtime lib.
	PlaceholderMigration<Pallet<T, I>, 10>,
);
