use crate::{mock::*, EarnedRelayerFees, Pallet, Swap, SwapQueue, WeightInfo};
use cf_chains::eth::assets;
use cf_primitives::{Asset, EthereumAddress, ForeignChainAddress};
use cf_traits::SwapIntentHandler;
use frame_support::{assert_noop, assert_ok};

use frame_support::traits::Hooks;

// Returns some test data
fn generate_test_swaps() -> Vec<Swap<u64>> {
	vec![
		Swap {
			from: Asset::Flip,
			to: Asset::Usdc,
			amount: 100,
			egress_address: ForeignChainAddress::Eth([2; 20]),
			relayer_id: 2_u64,
			relayer_commission_bps: 2000,
		},
		Swap {
			from: Asset::Flip,
			to: Asset::Usdc,
			amount: 200,
			egress_address: ForeignChainAddress::Eth([4; 20]),
			relayer_id: 3_u64,
			relayer_commission_bps: 2000,
		},
		Swap {
			from: Asset::Flip,
			to: Asset::Usdc,
			amount: 300,
			egress_address: ForeignChainAddress::Eth([7; 20]),
			relayer_id: 4_u64,
			relayer_commission_bps: 2000,
		},
		Swap {
			from: Asset::Eth,
			to: Asset::Usdc,
			amount: 40,
			egress_address: ForeignChainAddress::Eth([9; 20]),
			relayer_id: 5_u64,
			relayer_commission_bps: 2000,
		},
		Swap {
			from: Asset::Flip,
			to: Asset::Eth,
			amount: 500,
			egress_address: ForeignChainAddress::Eth([2; 20]),
			relayer_id: 6_u64,
			relayer_commission_bps: 2000,
		},
		Swap {
			from: Asset::Flip,
			to: Asset::Eth,
			amount: 600,
			egress_address: ForeignChainAddress::Eth([4; 20]),
			relayer_id: 7_u64,
			relayer_commission_bps: 2000,
		},
	]
}

fn insert_swaps(swaps: Vec<Swap<u64>>) {
	for swap in swaps.iter() {
		assert_ok!(<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			swap.from,
			swap.to,
			swap.amount,
			swap.egress_address,
			swap.relayer_id,
			2,
		));
	}
}

#[test]
fn register_swap_intent_success_with_valid_parameters() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::register_swap_intent(
			Origin::signed(ALICE),
			Asset::Eth,
			Asset::Usdc,
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
		Swapping::on_idle(
			1,
			<() as WeightInfo>::execute_group_of_swaps(swaps.len() as u32) * (swaps.len() as u64),
		);
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
		let expected = swaps
			.iter()
			.map(|swap: &Swap<u64>| EgressTransaction {
				asset: assets::eth::Asset::try_from(swap.to).unwrap(),
				amount: swap.amount,
				egress_address: EthereumAddress::try_from(swap.egress_address).unwrap().into(),
			})
			.collect::<Vec<EgressTransaction>>();
		for swap in expected.iter() {
			assert!(EgressQueue::<Test>::get()
				.expect("EgressQueue to not be empty")
				.contains(swap));
		}
	});
}

#[test]
fn number_of_swaps_processed_limited_by_weight() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(swaps);
		Swapping::on_idle(1, 200);
		assert_eq!(SwapQueue::<Test>::get().len(), 3);
	});
}

#[test]
fn expect_earned_fees_to_be_recorded() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 2_u64;
		const BOB: u64 = 3_u64;
		assert_ok!(<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			200,
		));
		assert_ok!(<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			Asset::Flip,
			Asset::Usdc,
			500,
			ForeignChainAddress::Eth([2; 20]),
			BOB,
			100,
		));
		Swapping::on_idle(1, 1000);
		assert_eq!(EarnedRelayerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 2);
		assert_eq!(EarnedRelayerFees::<Test>::get(BOB, cf_primitives::Asset::Flip), 5);
		assert_ok!(<Pallet<Test> as SwapIntentHandler>::schedule_swap(
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			200,
		));
		Swapping::on_idle(1, 1000);
		assert_eq!(EarnedRelayerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 4);
	});
}

#[test]
fn cannot_swap_with_incorrect_egress_address_type() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 1_u64;
		assert_noop!(
			<Pallet<Test> as SwapIntentHandler>::schedule_swap(
				Asset::Flip,
				Asset::Usdc,
				10,
				ForeignChainAddress::Dot([2; 32]),
				ALICE,
				2,
			),
			crate::Error::<Test>::IncompatibleAssetAndAddress,
		);
		assert_noop!(
			<Pallet<Test> as SwapIntentHandler>::schedule_swap(
				Asset::Flip,
				Asset::Eth,
				10,
				ForeignChainAddress::Dot([2; 32]),
				ALICE,
				2,
			),
			crate::Error::<Test>::IncompatibleAssetAndAddress,
		);
		assert_noop!(
			<Pallet<Test> as SwapIntentHandler>::schedule_swap(
				Asset::Eth,
				Asset::Flip,
				10,
				ForeignChainAddress::Dot([2; 32]),
				ALICE,
				2,
			),
			crate::Error::<Test>::IncompatibleAssetAndAddress,
		);
		assert_noop!(
			<Pallet<Test> as SwapIntentHandler>::schedule_swap(
				Asset::Flip,
				Asset::Dot,
				10,
				ForeignChainAddress::Eth([2; 20]),
				ALICE,
				2,
			),
			crate::Error::<Test>::IncompatibleAssetAndAddress,
		);
	});
}

#[test]
fn expect_swap_id_to_be_emitted() {
	new_test_ext().execute_with(|| {
		// 1. Register a swap intent -> NewSwapIntent
		// 2. Schedule the swap -> SwapIngressReceived
		// 3. Process swaps -> SwapEgressScheduled
	});
}
