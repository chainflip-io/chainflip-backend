use crate::{mock::*, Pallet, SwapQueue, WeightInfo};
use cf_primitives::{Asset, AssetAmount, ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::SwapIntentHandler;
use frame_support::assert_ok;

use frame_support::traits::Hooks;

fn insert_swaps(number_of_swaps: usize) {
	for i in 0..number_of_swaps {
		<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			Asset::Eth,
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			i as u128, /* we use the amount to make a distinctions between the
			            * different swaps in the queue */
			ForeignChainAddress::Eth(Default::default()),
			ForeignChainAddress::Eth(Default::default()),
		);
	}
	let swaps = SwapQueue::<Test>::get();
	assert_eq!(swaps.len(), number_of_swaps);
}

#[test]
fn register_swap_intent_success_with_valid_parameters() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::register_swap_intent(
			Origin::signed(ALICE),
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Eth },
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			ForeignChainAddress::Eth(Default::default()),
			0,
		));
	});
}

#[test]
fn process_all_swaps() {
	new_test_ext().execute_with(|| {
		const NUMBER_OF_SWAPS: usize = 10;
		insert_swaps(NUMBER_OF_SWAPS);
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * (NUMBER_OF_SWAPS as u64));
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
	});
}

#[test]
fn number_of_swaps_processed_limited_by_weight() {
	new_test_ext().execute_with(|| {
		insert_swaps(10);
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 8);
		assert_eq!(SwapQueue::<Test>::get().len(), 2);
		// Expect Swaps to be egressed in the right order
		assert_eq!(EgressQueue::<Test>::get().unwrap(), vec![0, 1, 2, 3, 4, 5, 6, 7]);
	});
}

#[test]
fn ensure_order_of_swap_processing() {
	new_test_ext().execute_with(|| {
		insert_swaps(10);
		// Let's process 5/10 swaps.
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 5);
		let remaining_swaps = SwapQueue::<Test>::get();
		assert_eq!(remaining_swaps.len(), 5);
		assert_eq!(
			remaining_swaps.iter().map(|el| el.amount).collect::<Vec<AssetAmount>>(),
			vec![5, 6, 7, 8, 9]
		);
		assert_eq!(EgressQueue::<Test>::get().unwrap(), vec![0, 1, 2, 3, 4]);
	});
}
