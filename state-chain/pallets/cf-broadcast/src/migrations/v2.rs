use crate::*;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::Vec;

mod old {

	use super::*;

	#[frame_support::storage_alias]
	pub type RequestCallbacks<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use frame_support::storage::StoragePrefixedMap;
		frame_support::migration::move_prefix(
			old::RequestCallbacks::<T, I>::storage_prefix(),
			RequestSuccessCallbacks::<T, I>::storage_prefix(),
		);

		let pending_broadcasts = AwaitingBroadcast::<T, I>::iter_keys()
			.map(|BroadcastAttemptId { broadcast_id, .. }| broadcast_id)
			.collect::<BTreeSet<_>>();
		PendingBroadcasts::<T, I>::put(pending_broadcasts);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let pending_broadcasts = PendingBroadcasts::<T, I>::get();
		for id in AwaitingBroadcast::<T, I>::iter_keys() {
			assert!(pending_broadcasts.contains(&id.broadcast_id),);
		}
		Ok(())
	}
}
