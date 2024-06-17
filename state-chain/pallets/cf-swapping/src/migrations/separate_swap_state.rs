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

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {

	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock::{new_test_ext, Test};

		new_test_ext().execute_with(|| {
			const FROM_ASSET: Asset = Asset::Flip;
			const TO_ASSET: Asset = Asset::Usdc;
			const INPUT_AMOUNT: AssetAmount = 100;
			let swap_type = SwapType::Swap(ForeignChainAddress::Eth([2; 20].into()));

			let swap_old = |swap_id: SwapId| old::Swap {
				swap_id,
				from: FROM_ASSET,
				to: TO_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: swap_type.clone(),
				stable_amount: Some(100),
				final_output: None,
			};

			let swap_new = |swap_id: SwapId| Swap {
				swap_id,
				from: FROM_ASSET,
				to: TO_ASSET,
				input_amount: INPUT_AMOUNT,
				swap_type: swap_type.clone(),
				refund_params: None,
			};

			old::SwapQueue::<Test>::insert(0, vec![swap_old(1), swap_old(2)]);
			old::SwapQueue::<Test>::insert(1, vec![swap_old(3)]);

			Migration::<Test>::on_runtime_upgrade();

			assert_eq!(SwapQueue::<Test>::get(0), vec![swap_new(1), swap_new(2),]);
			assert_eq!(SwapQueue::<Test>::get(1), vec![swap_new(3)]);
		});
	}
}
