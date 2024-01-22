use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::{marker::PhantomData, vec::Vec};

pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	use cf_primitives::ChannelId;
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub type SwapTTL<T: Config> = StorageValue<Pallet<T>, BlockNumberFor<T>, ValueQuery>;

	#[frame_support::storage_alias]
	pub type SwapChannelExpiries<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<(ChannelId, ForeignChainAddress)>,
		ValueQuery,
	>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let _ = old::SwapChannelExpiries::<T>::drain().collect::<Vec<_>>();

		let _ = old::SwapTTL::<T>::take();

		Weight::zero()
	}
}
