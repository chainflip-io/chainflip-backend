pub mod v3;

use cf_runtime_upgrade_utilities::VersionedMigration;

pub type PalletMigration<T> = (VersionedMigration<crate::Pallet<T>, v3::Migration<T>, 2, 3>,);

pub mod old {
	use crate::*;
	use frame_support::{pallet_prelude::ValueQuery, Blake2_128Concat};

	#[frame_support::storage_alias]
	pub type ActiveBidder<T: crate::Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		bool,
		ValueQuery,
	>;
}
