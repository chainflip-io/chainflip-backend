use crate::{mock::*, Pallet, SwapQueue, WeightInfo};
use cf_primitives::{Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::SwapIntentHandler;
use frame_support::assert_ok;

use frame_support::traits::Hooks;

fn insert_swaps(amount: usize) {
	for i in 0..amount {
		<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			Asset::Eth,
			ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			i as u128, /* we use the amount to make a distinctions between the
			            * different swaps in the queue */
			ForeignChainAddress::Eth(Default::default()),
			ForeignChainAddress::Eth(Default::default()),
		);
	}
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
fn swap_processing() {
	new_test_ext().execute_with(|| {
		const NUMBER_OF_SWAPS: usize = 10;
		insert_swaps(NUMBER_OF_SWAPS);
		// Expect that we process all swaps if we have enough weight
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * (NUMBER_OF_SWAPS as u64));
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
		// Expect that we only process 8 of 10 swaps if we have limited weight for that
		insert_swaps(NUMBER_OF_SWAPS);
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 8);
		assert_eq!(SwapQueue::<Test>::get().len(), 2);
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
		// Expect the latest added swaps still in the queue.
		assert_eq!(5, remaining_swaps.get(0).unwrap().amount);
		assert_eq!(9, remaining_swaps.get(4).unwrap().amount);
	});
}
