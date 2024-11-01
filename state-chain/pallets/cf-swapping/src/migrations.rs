use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T> = PlaceholderMigration<6, Pallet<T>>;
