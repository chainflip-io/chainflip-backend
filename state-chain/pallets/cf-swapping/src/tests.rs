use crate::{mock::*, Pallet, Swap, SwapQueue, WeightInfo};
use cf_primitives::{Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::SwapIntentHandler;
use frame_support::assert_ok;

use frame_support::traits::Hooks;

// Returns some test data
fn generate_test_swaps() -> Vec<Swap> {
	vec![
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 10,
			ingress_address: ForeignChainAddress::Eth([1; 20]),
			egress_address: ForeignChainAddress::Eth([2; 20]),
		},
		Swap {
			from: Asset::Usdc,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Flip },
			amount: 20,
			ingress_address: ForeignChainAddress::Eth([3; 20]),
			egress_address: ForeignChainAddress::Eth([4; 20]),
		},
		Swap {
			from: Asset::Eth,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 30,
			ingress_address: ForeignChainAddress::Eth([5; 20]),
			egress_address: ForeignChainAddress::Eth([7; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 40,
			ingress_address: ForeignChainAddress::Eth([9; 20]),
			egress_address: ForeignChainAddress::Eth([9; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 50,
			ingress_address: ForeignChainAddress::Eth([1; 20]),
			egress_address: ForeignChainAddress::Eth([2; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 60,
			ingress_address: ForeignChainAddress::Eth([3; 20]),
			egress_address: ForeignChainAddress::Eth([4; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 70,
			ingress_address: ForeignChainAddress::Eth([5; 20]),
			egress_address: ForeignChainAddress::Eth([6; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 80,
			ingress_address: ForeignChainAddress::Eth([7; 20]),
			egress_address: ForeignChainAddress::Eth([8; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 90,
			ingress_address: ForeignChainAddress::Eth([9; 20]),
			egress_address: ForeignChainAddress::Eth([5; 20]),
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 100,
			ingress_address: ForeignChainAddress::Eth([1; 20]),
			egress_address: ForeignChainAddress::Eth([2; 20]),
		},
	]
}

fn insert_swaps(swaps: Vec<Swap>) {
	for swap in swaps.iter() {
		<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			swap.from,
			swap.to,
			swap.amount,
			swap.ingress_address,
			swap.egress_address,
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
fn process_all_swaps() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(swaps.clone());
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * (swaps.len() as u64));
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
	});
}

#[test]
fn number_of_swaps_processed_limited_by_weight() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(swaps.clone());
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 8);
		// Expect 2 swaps left in the SwapQueue.
		assert_eq!(SwapQueue::<Test>::get().len(), 2);
	});
}

#[test]
fn ensure_order_of_swap_processing() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(swaps.clone());
		// Let's process 5/10 swaps.
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 5);
		let remaining_swaps = SwapQueue::<Test>::get();
		assert_eq!(remaining_swaps.len(), 5);
		// Expect the first five swaps to be executed and in the Storage of the Mock.
		assert_eq!(
			EgressQueue::<Test>::get().unwrap(),
			swaps[0..5]
				.iter()
				.map(|swap: &Swap| EgressTransaction {
					foreign_asset: swap.to,
					amount: swap.amount,
					egress_address: swap.egress_address,
				})
				.collect::<Vec<EgressTransaction>>()
		);
	});
}
