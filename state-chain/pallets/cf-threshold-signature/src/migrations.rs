use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

pub mod migrate_signature_to_include_signer;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, migrate_signature_to_include_signer::Migration<T, I>, 5, 6>,
	PlaceholderMigration<Pallet<T, I>, 6>,
);
