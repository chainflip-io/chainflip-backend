use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	use frame_support::pallet_prelude::ValueQuery;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct Swap {
		pub swap_id: u64,
		pub from: Asset,
		pub to: Asset,
		pub amount: AssetAmount,
		pub swap_type: SwapType,
		pub stable_amount: Option<AssetAmount>,
		pub final_output: Option<AssetAmount>,
		pub fee_taken: bool,
	}

	#[frame_support::storage_alias]
	pub(crate) type SwapQueue<T: Config> = StorageValue<Pallet<T>, Vec<Swap>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let existing_swaps = old::SwapQueue::<T>::take();

		let current_block = frame_system::Pallet::<T>::block_number();

		let updated_swaps: Vec<_> = existing_swaps
			.into_iter()
			.map(|s| Swap::<T> {
				swap_id: s.swap_id,
				from: s.from,
				to: s.to,
				amount: s.amount,
				swap_type: s.swap_type,
				stable_amount: s.stable_amount,
				final_output: s.final_output,
				fee_taken: s.fee_taken,
				execute_at: current_block,
			})
			.collect();

		SwapQueue::<T>::put(updated_swaps);

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
		ensure!(
			pre_upgrade_count == SwapQueue::<T>::decode_len().unwrap_or_default() as u32,
			"Swap count mismatch!"
		);
		Ok(())
	}
}
