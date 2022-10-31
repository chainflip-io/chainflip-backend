use crate::{mock::*, Pallet, Swap, SwapQueue, WeightInfo};
use cf_primitives::{Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::SwapIntentHandler;
use frame_support::assert_ok;

use frame_support::traits::Hooks;

// Returns some test data
fn generate_test_swaps() -> Vec<Swap<u64>> {
	vec![
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 10,
			egress_address: ForeignChainAddress::Eth([2; 20]),
			relayer_id: 2_u64,
		},
		Swap {
			from: Asset::Usdc,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Flip },
			amount: 20,
			egress_address: ForeignChainAddress::Eth([4; 20]),
			relayer_id: 3_u64,
		},
		Swap {
			from: Asset::Eth,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 30,
			egress_address: ForeignChainAddress::Eth([7; 20]),
			relayer_id: 4_u64,
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 40,
			egress_address: ForeignChainAddress::Eth([9; 20]),
			relayer_id: 5_u64,
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 50,
			egress_address: ForeignChainAddress::Eth([2; 20]),
			relayer_id: 6_u64,
		},
		Swap {
			from: Asset::Flip,
			to: ForeignChainAsset { chain: ForeignChain::Ethereum, asset: Asset::Usdc },
			amount: 60,
			egress_address: ForeignChainAddress::Eth([4; 20]),
			relayer_id: 7_u64,
		},
	]
}

fn insert_swaps(swaps: Vec<Swap<u64>>) {
	for swap in swaps.iter() {
		<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			swap.from,
			swap.to,
			swap.amount,
			swap.egress_address,
			swap.relayer_id,
			2,
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
		assert_eq!(
			EgressQueue::<Test>::get().unwrap(),
			swaps
				.iter()
				.map(|swap: &Swap<u64>| EgressTransaction {
					foreign_asset: swap.to,
					amount: swap.amount,
					egress_address: swap.egress_address,
				})
				.collect::<Vec<EgressTransaction>>()
		);
	});
}

#[test]
fn number_of_swaps_processed_limited_by_weight() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(swaps.clone());
		Swapping::on_idle(1, <() as WeightInfo>::execute_swap() * 3);
		assert_eq!(SwapQueue::<Test>::get().len(), 3);
		assert_eq!(
			EgressQueue::<Test>::get().unwrap(),
			swaps[0..3]
				.iter()
				.map(|swap: &Swap<u64>| EgressTransaction {
					foreign_asset: swap.to,
					amount: swap.amount,
					egress_address: swap.egress_address,
				})
				.collect::<Vec<EgressTransaction>>()
		);
	});
}
