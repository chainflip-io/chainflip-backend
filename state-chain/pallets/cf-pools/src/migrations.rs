pub mod rename_slippage_to_price_impact;

use crate::Pallet;
use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> =
	(VersionedMigration<Pallet<T>, rename_slippage_to_price_impact::Migration<T>, 3, 4>,);
