use frame_support::traits::OnRuntimeUpgrade;

use crate::*;
use core::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct Swap {
		pub swap_id: SwapId,
		pub from: Asset,
		pub to: Asset,
		pub input_amount: AssetAmount,
		pub swap_type: SwapType,
		pub stable_amount: Option<AssetAmount>,
		pub final_output: Option<AssetAmount>,
	}

	#[frame_support::storage_alias]
	pub type SwapQueue<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, BlockNumberFor<T>, Vec<Swap>, ValueQuery>;

	#[frame_support::storage_alias]
	pub type FirstUnprocessedBlock<T: Config> =
		StorageValue<Pallet<T>, BlockNumberFor<T>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		SwapQueue::<T>::translate(|_, old_swaps: Vec<old::Swap>| {
			Some(
				old_swaps
					.into_iter()
					.map(|old_swap| Swap {
						swap_id: old_swap.swap_id,
						from: old_swap.from,
						to: old_swap.to,
						input_amount: old_swap.input_amount,
						refund_params: None,
						swap_type: old_swap.swap_type,
					})
					.collect::<Vec<_>>(),
			)
		});

		let current_block = frame_system::Pallet::<T>::block_number();

		// If any swaps are schedules for swaps in the past, we reschedule
		// them to the current block instead. Note that migrations are executed
		// at the beginning of a block, so swaps for `current_block` haven't been
		// processed yet.
		for block_number in SwapQueue::<T>::iter_keys() {
			if block_number < current_block {
				for swap in SwapQueue::<T>::take(block_number) {
					SwapQueue::<T>::append(current_block, swap);
				}
			}
		}

		// No longer needed:
		old::FirstUnprocessedBlock::<T>::kill();

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {

	use super::*;

	const FROM_ASSET: Asset = Asset::Flip;
	const TO_ASSET: Asset = Asset::Usdc;
	const INPUT_AMOUNT: AssetAmount = 100;

	fn swap_old(swap_id: SwapId) -> old::Swap {
		old::Swap {
			swap_id,
			from: FROM_ASSET,
			to: TO_ASSET,
			input_amount: INPUT_AMOUNT,
			swap_type: SwapType::Swap(ForeignChainAddress::Eth([2; 20].into())),
			stable_amount: Some(100),
			final_output: None,
		}
	}

	fn swap_new(swap_id: SwapId) -> Swap {
		Swap {
			swap_id,
			from: FROM_ASSET,
			to: TO_ASSET,
			input_amount: INPUT_AMOUNT,
			swap_type: SwapType::Swap(ForeignChainAddress::Eth([2; 20].into())),
			refund_params: None,
		}
	}

	#[test]
	fn test_migration() {
		use crate::mock::{new_test_ext, Test};

		new_test_ext().then_execute_at_block(10u64, |_| {
			// Note that some swaps are scheduled for blocks in the past:
			old::SwapQueue::<Test>::insert(5, vec![swap_old(1), swap_old(2)]);
			old::SwapQueue::<Test>::insert(9, vec![swap_old(3)]);
			old::SwapQueue::<Test>::insert(10, vec![swap_old(4)]);
			old::SwapQueue::<Test>::insert(11, vec![swap_old(5)]);

			Migration::<Test>::on_runtime_upgrade();

			// Now all swaps are scheduled for the current or future blocks:
			assert_eq!(SwapQueue::<Test>::get(5), vec![]);
			assert_eq!(SwapQueue::<Test>::get(9), vec![]);
			assert_eq!(
				SwapQueue::<Test>::get(10),
				vec![swap_new(4), swap_new(1), swap_new(2), swap_new(3)]
			);
			assert_eq!(SwapQueue::<Test>::get(11), vec![swap_new(5)]);
		});
	}
}
