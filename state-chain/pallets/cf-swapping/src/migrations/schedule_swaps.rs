use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub(crate) type SwapQueue<T: Config> = StorageValue<Pallet<T>, Vec<Swap>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_block = frame_system::Pallet::<T>::block_number();

		FirstUnprocessedBlock::<T>::set(current_block);

		SwapQueue::<T>::insert(current_block, old::SwapQueue::<T>::take());

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_pending_swaps = old::SwapQueue::<T>::decode_len().unwrap_or_default() as u32;
		Ok(number_pending_swaps.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		let pre_upgrade_count =
			<u32>::decode(&mut &state[..]).map_err(|_| "Failed to decode pre-upgrade state.")?;

		let current_block = frame_system::Pallet::<T>::block_number();
		ensure!(
			pre_upgrade_count == SwapQueue::<T>::get(current_block).len() as u32,
			"Swap count mismatch!"
		);
		ensure!(
			SwapQueue::<T>::iter_keys().count() == 1,
			"All swaps existing swaps should be scheduled at the same block"
		);
		Ok(())
	}
}
