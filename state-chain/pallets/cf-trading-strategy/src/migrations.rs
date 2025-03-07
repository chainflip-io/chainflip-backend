use cf_runtime_utilities::PlaceholderMigration;

use crate::Pallet;

pub type PalletMigration<T> = (PlaceholderMigration<1, Pallet<T>>,);
