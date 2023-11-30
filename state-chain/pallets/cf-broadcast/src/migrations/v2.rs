use crate::*;
use frame_support::{
	migration, pallet_prelude::Weight, traits::OnRuntimeUpgrade, StoragePrefixedMap,
};
use sp_std::marker::PhantomData;

mod old {
	use super::*;
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};

	#[frame_support::storage_alias]
	pub type RequestCallbacks<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		BroadcastId,
		<T as Config<I>>::BroadcastCallable,
		OptionQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		migration::move_prefix(
			old::RequestCallbacks::<T, I>::storage_prefix(),
			RequestSuccessCallbacks::<T, I>::storage_prefix(),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		let count = old::RequestCallbacks::<T, I>::iter().count() as u32;
		Ok(count.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		let old_count = u32::decode(&mut &*state).expect("Invalid data passed from pre_upgrade");
		frame_support::ensure!(
			old_count == RequestSuccessCallbacks::<T, I>::iter().count() as u32,
			"Count mismatch"
		);
		Ok(())
	}
}
