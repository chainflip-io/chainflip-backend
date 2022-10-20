use crate::{mock::*, SwapQueue, WeightInfo};
use cf_primitives::{Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset, Swap};
use frame_support::assert_ok;

use frame_support::traits::Hooks;

fn generate_swaps(amount: usize) -> Vec<Swap> {
	let mut swaps: Vec<Swap> = vec![];
	for i in 0..amount {
		swaps.push(Swap {
			from: Asset::Eth,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: i as u128, /* we use the amount to make a distinctions between the different
			                    * swaps in the queue */
			ingress_address: ForeignChainAddress::Eth(Default::default()),
			egress_address: ForeignChainAddress::Eth(Default::default()),
		});
	}
	swaps
}

#[test]
fn request_swap_intent() {
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
		const SWAP_AMOUNT: usize = 10;
		SwapQueue::<Test>::put(generate_swaps(SWAP_AMOUNT));
		// Expect that we process all swaps if we have enough weight
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * (SWAP_AMOUNT as u64));
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
		// Expect that we only process 8 of 10 swaps if we have limited weight for that
		SwapQueue::<Test>::put(generate_swaps(SWAP_AMOUNT));
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 8);
		assert_eq!(SwapQueue::<Test>::get().len(), 2);
		// Expect to have 5 Swaps left if we have only weight for 10
		SwapQueue::<Test>::put(generate_swaps(SWAP_AMOUNT + 5));
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 10);
		assert_eq!(SwapQueue::<Test>::get().len(), 5);
	});
}

#[test]
fn ensure_order_of_swap_processing() {
	new_test_ext().execute_with(|| {
		const SWAP_AMOUNT: usize = 10;
		SwapQueue::<Test>::put(generate_swaps(SWAP_AMOUNT));
		let swaps = SwapQueue::<Test>::get();
		// Expect the initial swaps to be in the right order.
		assert_eq!(0, swaps.get(0).unwrap().amount);
		assert_eq!(9, swaps.get(9).unwrap().amount);
		// Let's process 5/10 swaps.
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 5);
		let left_swaps = SwapQueue::<Test>::get();
		assert_eq!(left_swaps.len(), 5);
		// Expect the latest added swaps still in the queue.
		assert_eq!(5, left_swaps.get(0).unwrap().amount);
		assert_eq!(9, left_swaps.get(4).unwrap().amount);
	});
}
