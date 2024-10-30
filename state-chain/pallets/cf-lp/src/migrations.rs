use crate::Pallet;
use cf_runtime_upgrade_utilities::PlaceholderMigration;

pub type PalletMigration<T> = PlaceholderMigration<3, Pallet<T>>;
