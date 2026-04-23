use crate::{Pallet, PALLET_VERSION};
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T> = (PlaceholderMigration<{ PALLET_VERSION }, Pallet<T>>,);

#[cfg(test)]
const _: u16 = <PalletMigration<crate::mock::Test> as cf_runtime_utilities::MigrationSequence>::FROM;
