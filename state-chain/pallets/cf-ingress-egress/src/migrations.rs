use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod add_owner_to_channel_details;
mod rename_tx_reports;
mod update_rejection_params;

pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_owner_to_channel_details::Migration<T, I>, 15, 16>,
	VersionedMigration<Pallet<T, I>, rename_tx_reports::Migration<T, I>, 16, 17>,
	VersionedMigration<Pallet<T, I>, update_rejection_params::Migration<T, I>, 17, 18>,
	PlaceholderMigration<Pallet<T, I>, 18>,
);
