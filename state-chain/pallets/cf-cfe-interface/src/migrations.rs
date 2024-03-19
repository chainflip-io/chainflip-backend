pub mod add_arb_to_cfe_events;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> =
	(VersionedMigration<crate::Pallet<T>, add_arb_to_cfe_events::Migration<T>, 0, 1>,);
