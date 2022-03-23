pub(crate) mod add_mint_interval;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub(crate) type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, add_mint_interval::Migration<T>, 0, 1>,);
