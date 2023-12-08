use crate::*;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::Vec;

mod old {
	use super::*;

	/// A unique id for each broadcast attempt
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default, Copy)]
	pub struct BroadcastAttemptId {
		pub broadcast_id: BroadcastId,
		pub attempt_count: AttemptCount,
	}

	#[frame_support::storage_alias]
	pub type RequestCallbacks<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;

	#[frame_support::storage_alias]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		BroadcastAttemptId,
		TransactionSigningAttempt<T, I>,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use frame_support::storage::StoragePrefixedMap;

		// Renaming of storage: RequestCallbacks -> RequestSuccessCallbacks
		frame_support::migration::move_prefix(
			old::RequestCallbacks::<T, I>::storage_prefix(),
			RequestSuccessCallbacks::<T, I>::storage_prefix(),
		);

		// Adding Awaiting Broadcasts -> PendingBroadcasts
		let pending_broadcasts = old::AwaitingBroadcast::<T, I>::iter_keys()
			.map(|old::BroadcastAttemptId { broadcast_id, .. }| broadcast_id)
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
