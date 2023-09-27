use crate::*;
#[cfg(feature = "try-runtime")]
use frame_support::dispatch::DispatchError;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;

mod old {
	use frame_support::pallet_prelude::OptionQuery;

	use super::*;

	#[frame_support::storage_alias]
	pub type TransactionOutIdToBroadcastId<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, TransactionOutIdFor<T, I>, BroadcastId, OptionQuery>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let chain_height = T::ChainTracking::get_block_height();

		TransactionOutIdToBroadcastId::<T, I>::translate::<BroadcastId, _>(|_id, old| {
			Some((old, chain_height))
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		use frame_support::ensure;

		let chain_height = T::ChainTracking::get_block_height();
		// If it's at 0 something went wrong with the initialisation. Also since initiated_at is the
		// last thing being decoded, this acts as a check that the rest of the decoding worked.
		ensure!(chain_height > 0u32.into(), "chain_height is 0");
		Ok(chain_height.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use frame_support::ensure;

		let pre_upgrade_height = ChainBlockNumberFor::<T, I>::decode(&mut &state[..])
			.map_err(|_| "Failed to decode pre-upgrade state.")?;

		for (_out_id, (_b_id, initiated_at)) in TransactionOutIdToBroadcastId::<T, I>::iter() {
			ensure!(initiated_at >= pre_upgrade_height, "initiated_at is 0");
		}
		Ok(())
	}
}
