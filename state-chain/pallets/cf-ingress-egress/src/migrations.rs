pub mod btc_deposit_channels;
pub mod deposit_channels_with_boost_fee;
pub mod set_dust_limit;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T, I> = (
	VersionedMigration<crate::Pallet<T, I>, btc_deposit_channels::Migration<T, I>, 2, 3>,
	VersionedMigration<crate::Pallet<T, I>, set_dust_limit::Migration<T, I>, 3, 4>,
	VersionedMigration<crate::Pallet<T, I>, deposit_channels_with_boost_fee::Migration<T, I>, 4, 5>,
);
