use crate::{mock::*, EarnedRelayerFees, Error, Pallet, Swap, SwapQueue, WeightInfo};
use cf_chains::{address::ForeignChainAddress, AnyChain};
use cf_primitives::{Asset, ForeignChain};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{mocks::egress_handler::MockEgressHandler, SwapIntentHandler};
use frame_support::{assert_noop, assert_ok, sp_std::iter, weights::Weight};

use frame_support::traits::Hooks;

// Returns some test data
fn generate_test_swaps() -> Vec<Swap> {
	vec![
		// asset -> USDC
		Swap {
			swap_id: 1,
			from: Asset::Flip,
			to: Asset::Usdc,
			amount: 100,
			egress_address: ForeignChainAddress::Eth([2; 20]),
		},
		// USDC -> asset
		Swap {
			swap_id: 2,
			from: Asset::Eth,
			to: Asset::Usdc,
			amount: 40,
			egress_address: ForeignChainAddress::Eth([9; 20]),
		},
		// Both assets are on the Eth chain
		Swap {
			swap_id: 3,
			from: Asset::Flip,
			to: Asset::Eth,
			amount: 500,
			egress_address: ForeignChainAddress::Eth([2; 20]),
		},
		// Cross chain
		Swap {
			swap_id: 4,
			from: Asset::Flip,
			to: Asset::Dot,
			amount: 600,
			egress_address: ForeignChainAddress::Dot([4; 32]),
		},
	]
}

fn insert_swaps(swaps: &[Swap]) {
	for (relayer_id, swap) in swaps.iter().enumerate() {
		<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
			ForeignChainAddress::Eth([2; 20]),
			swap.from,
			swap.to,
			swap.amount,
			swap.egress_address.clone(),
			relayer_id as u64,
			2,
		);
	}
}

#[test]
fn register_swap_intent_success_with_valid_parameters() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(ALICE),
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
		insert_swaps(&swaps);
		Swapping::on_idle(
			1,
			<() as WeightInfo>::execute_group_of_swaps(swaps.len() as u32) * (swaps.len() as u64),
		);
		assert!(SwapQueue::<Test>::get().is_empty());
		let mut expected = swaps
			.iter()
			.cloned()
			.map(|swap| (swap.to, swap.amount, swap.egress_address))
			.collect::<Vec<_>>();
		expected.sort();
		let mut egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		egresses.sort();
		for (input, output) in iter::zip(expected, egresses) {
			assert_eq!(input, output);
		}
	});
}

#[test]
fn number_of_swaps_processed_limited_by_weight() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(&swaps);
		Swapping::on_idle(1, Weight::from_ref_time(0));
		assert_eq!(SwapQueue::<Test>::get().len(), 0);
	});
}

#[test]
fn expect_earned_fees_to_be_recorded() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 2_u64;
		<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
			ForeignChainAddress::Eth([2; 20]),
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			200,
		);
		Swapping::on_idle(1, Weight::from_ref_time(1000));
		assert_eq!(EarnedRelayerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 2);
		<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
			ForeignChainAddress::Eth([2; 20]),
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			200,
		);
		Swapping::on_idle(1, Weight::from_ref_time(1000));
		assert_eq!(EarnedRelayerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 4);
	});
}

#[test]
#[should_panic]
fn cannot_swap_with_incorrect_egress_address_type() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 1_u64;
		<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
			ForeignChainAddress::Eth([2; 20]),
			Asset::Eth,
			Asset::Dot,
			10,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			2,
		);
		assert_eq!(SwapQueue::<Test>::get(), vec![]);
	});
}

#[test]
fn expect_swap_id_to_be_emitted() {
	new_test_ext().execute_with(|| {
		// 1. Register a swap intent -> NewSwapIntent
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			Asset::Usdc,
			ForeignChainAddress::Eth(Default::default()),
			0,
		));
		// 2. Schedule the swap -> SwapIngressReceived
		<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
			ForeignChainAddress::Eth(Default::default()),
			Asset::Flip,
			Asset::Usdc,
			500,
			ForeignChainAddress::Eth(Default::default()),
			ALICE,
			0,
		);
		// 3. Process swaps -> SwapExecuted, SwapEgressScheduled
		Swapping::on_idle(1, Weight::from_ref_time(100));
		assert_event_sequence!(
			Test,
			crate::mock::RuntimeEvent::Swapping(crate::Event::NewSwapIntent {
				ingress_address: ForeignChainAddress::Eth(Default::default()),
			}),
			crate::mock::RuntimeEvent::Swapping(crate::Event::SwapIngressReceived {
				ingress_address: ForeignChainAddress::Eth(Default::default()),
				swap_id: 1,
				ingress_amount: 500
			}),
			crate::mock::RuntimeEvent::Swapping(crate::Event::SwapExecuted { swap_id: 1 }),
			crate::mock::RuntimeEvent::Swapping(crate::Event::SwapEgressScheduled {
				swap_id: 1,
				egress_id: (ForeignChain::Ethereum, 1),
				asset: Asset::Usdc,
				amount: 500,
			})
		);
	});
}

#[test]
fn withdraw_relayer_fees() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				ForeignChainAddress::Eth(Default::default()),
			),
			<Error<Test>>::NoFundsAvailable
		);
		EarnedRelayerFees::<Test>::insert(ALICE, Asset::Eth, 200);
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
		));
		let mut egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		assert!(egresses.len() == 1);
		assert_eq!(egresses.pop().expect("must exist").1, 200);
		System::assert_last_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::WithdrawalRequested {
				egress_id: (ForeignChain::Ethereum, 1),
				amount: 200,
				address: ForeignChainAddress::Eth(Default::default()),
			},
		));
	});
}

#[test]
fn can_swap_using_witness_origin() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			Asset::Eth,
			Asset::Flip,
			1_000,
			ForeignChainAddress::Eth([0u8; 20]),
		));

		System::assert_last_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::SwapScheduledByWitnesser {
				swap_id: 1,
				ingress_amount: 1_000,
				egress_address: ForeignChainAddress::Eth([0u8; 20]),
			},
		));
	});
}
