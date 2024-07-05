use crate::Pallet;
use cf_runtime_upgrade_utilities::PlaceholderMigration;

mod authorities;

pub type PalletMigration<T> = PlaceholderMigration<Pallet<T>, 3>;
