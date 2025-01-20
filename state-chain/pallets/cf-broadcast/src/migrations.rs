use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T, I> = (PlaceholderMigration<12, Pallet<T, I>>,);
