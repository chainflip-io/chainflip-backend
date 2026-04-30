use crate::{Pallet, STORAGE_VERSION_U16};
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T> = (PlaceholderMigration<{ STORAGE_VERSION_U16 }, Pallet<T>>,);

#[cfg(test)]
const _: u16 =
	<PalletMigration<crate::mock::Test> as cf_runtime_utilities::MigrationSequence>::FROM;
