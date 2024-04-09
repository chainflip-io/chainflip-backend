use crate::Pallet;
use cf_runtime_upgrade_utilities::PlaceholderMigration;

pub type PalletMigration<T> = PlaceholderMigration<Pallet<T>, 3>;

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
