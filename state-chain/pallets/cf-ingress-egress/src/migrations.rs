use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T, I> = (PlaceholderMigration<16, Pallet<T, I>>,);
