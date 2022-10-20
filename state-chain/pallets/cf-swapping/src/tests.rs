use crate::{mock::*, SwapQueue, WeightInfo};
use cf_primitives::{Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset, Swap};
use frame_support::assert_ok;

use frame_support::traits::Hooks;

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
fn process_swaps() {
	new_test_ext().execute_with(|| {
		const SWAP_AMOUNT: usize = 10;
		let swap = Swap {
			from: Asset::Eth,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 10,
			ingress_address: ForeignChainAddress::Eth(Default::default()),
			egress_address: ForeignChainAddress::Eth(Default::default()),
		};
		SwapQueue::<Test>::put([swap; SWAP_AMOUNT].to_vec());
		// Expect that we process all swaps if we have enough weight
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * (SWAP_AMOUNT as u64));
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
		// Expect that we only process 8 of 10 swaps if we have limited weight for that
		SwapQueue::<Test>::put([swap; SWAP_AMOUNT].to_vec());
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 8);
		assert_eq!(SwapQueue::<Test>::get().len(), 2);
		// Expect to have 5 Swaps left if we have only weight 10
		SwapQueue::<Test>::put([swap; SWAP_AMOUNT + 5].to_vec());
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 10);
		assert_eq!(SwapQueue::<Test>::get().len(), 5);
	});
}
