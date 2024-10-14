use crate::Pallet;
use cf_runtime_upgrade_utilities::{PlaceholderMigration, VersionedMigration};

mod add_owner_to_channel_details;
pub type PalletMigration<T, I> = (
	VersionedMigration<Pallet<T, I>, add_owner_to_channel_details::Migration<T, I>, 15, 16>,
	PlaceholderMigration<Pallet<T, I>, 16>,
);
