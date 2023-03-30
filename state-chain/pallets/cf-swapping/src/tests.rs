use crate::{
	mock::*, CcmGasBudget, CcmStage, CcmWithStages, EarnedRelayerFees, Error, Pallet, PendingCcms,
	Swap, SwapQueue, SwapType, WeightInfo,
};
use cf_chains::{address::ForeignChainAddress, AnyChain, CcmIngressMetadata};
use cf_primitives::{Asset, ForeignChain};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::egress_handler::{MockEgressHandler, MockEgressParameter},
	CcmHandler, SwapIntentHandler,
};
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
			swap_type: SwapType::Swap(ForeignChainAddress::Eth([2; 20])),
		},
		// USDC -> asset
		Swap {
			swap_id: 2,
			from: Asset::Eth,
			to: Asset::Usdc,
			amount: 40,
			swap_type: SwapType::Swap(ForeignChainAddress::Eth([9; 20])),
		},
		// Both assets are on the Eth chain
		Swap {
			swap_id: 3,
			from: Asset::Flip,
			to: Asset::Eth,
			amount: 500,
			swap_type: SwapType::Swap(ForeignChainAddress::Eth([2; 20])),
		},
		// Cross chain
		Swap {
			swap_id: 4,
			from: Asset::Flip,
			to: Asset::Dot,
			amount: 600,
			swap_type: SwapType::Swap(ForeignChainAddress::Dot([4; 32])),
		},
	]
}

fn insert_swaps(swaps: &[Swap]) {
	for (relayer_id, swap) in swaps.iter().enumerate() {
		if let SwapType::Swap(egress_address) = &swap.swap_type {
			<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
				ForeignChainAddress::Eth([2; 20]),
				swap.from,
				swap.to,
				swap.amount,
				egress_address.clone(),
				relayer_id as u64,
				2,
			);
		}
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
			None
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
			.map(|swap| MockEgressParameter::<AnyChain>::Swap {
				asset: swap.to,
				amount: swap.amount,
				egress_address: if let SwapType::Swap(egress_address) = swap.swap_type {
					egress_address
				} else {
					ForeignChainAddress::Eth(Default::default())
				},
			})
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
			None
		));
		// 2. Schedule the swap -> SwapIngressReceived
		<Pallet<Test> as SwapIntentHandler>::on_swap_ingress(
			ForeignChainAddress::Eth(Default::default()),
			Asset::Eth,
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
				ingress_amount: 500,
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
		assert_eq!(egresses.pop().expect("must exist").amount(), 200);
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

#[test]
fn can_process_ccms() {
	new_test_ext().execute_with(|| {
		let ccm_1 = CcmIngressMetadata {
			message: vec![0x01],
			gas_budget: 1_000,
			refund_address: ForeignChainAddress::Dot([0x01; 32]),
		};
		let ccm_2 = CcmIngressMetadata {
			message: vec![0x02],
			gas_budget: 2_000,
			refund_address: ForeignChainAddress::Dot([0x02; 32]),
		};

		// Can ingress CCM via Swap Intent
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(ALICE),
			Asset::Dot,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			0,
			Some(ccm_1.clone())
		),);
		assert_ok!(Swapping::on_ccm_ingress(
			Asset::Dot,
			2_000,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm_1.clone(),
		));

		// Can ingress CCM directly via Pallet Extrinsic.
		assert_ok!(Swapping::ccm_ingress(
			RuntimeOrigin::root(),
			Asset::Btc,
			1_000_000,
			Asset::Usdc,
			ForeignChainAddress::Eth(Default::default()),
			ccm_2.clone()
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmWithStages {
				ingress_asset: Asset::Dot,
				ingress_amount: 2_000,
				egress_asset: Asset::Eth,
				egress_address: ForeignChainAddress::Eth(Default::default()),
				message_metadata: ccm_1,
				stage: CcmStage::Ingressed { asset_swap_id: 1, gas_swap_id: 2 }
			})
		);
		assert_eq!(
			PendingCcms::<Test>::get(2),
			Some(CcmWithStages {
				ingress_asset: Asset::Btc,
				ingress_amount: 1_000_000,
				egress_asset: Asset::Usdc,
				egress_address: ForeignChainAddress::Eth(Default::default()),
				message_metadata: ccm_2,
				stage: CcmStage::Ingressed { asset_swap_id: 3, gas_swap_id: 4 }
			})
		);

		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![
				Swap {
					swap_id: 1,
					from: Asset::Dot,
					to: Asset::Eth,
					amount: 1_000,
					swap_type: SwapType::Ccm(1)
				},
				Swap {
					swap_id: 2,
					from: Asset::Dot,
					to: Asset::Eth,
					amount: 1_000,
					swap_type: SwapType::Ccm(1)
				},
				Swap {
					swap_id: 3,
					from: Asset::Btc,
					to: Asset::Usdc,
					amount: 998_000,
					swap_type: SwapType::Ccm(2)
				},
				Swap {
					swap_id: 4,
					from: Asset::Btc,
					to: Asset::Eth,
					amount: 2_000,
					swap_type: SwapType::Ccm(2)
				}
			]
		);

		// Swaps are executed during on_idle
		Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		// Both CCMs are scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![
				MockEgressParameter::Ccm {
					asset: Asset::Eth,
					amount: 1_000,
					egress_address: ForeignChainAddress::Eth(Default::default()),
					message: vec![0x01],
					refund_address: ForeignChainAddress::Dot([0x01; 32]),
				},
				MockEgressParameter::Ccm {
					asset: Asset::Usdc,
					amount: 998_000,
					egress_address: ForeignChainAddress::Eth(Default::default()),
					message: vec![0x02],
					refund_address: ForeignChainAddress::Dot([0x02; 32]),
				},
			]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, 1_000)));
		assert_eq!(CcmGasBudget::<Test>::get(2), Some((Asset::Eth, 2_000)));

		// Completed CCMs are removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(PendingCcms::<Test>::get(2), None);

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: 1,
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));
		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: 2,
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));
	});
}

#[test]
fn can_reject_invalid_ccms() {
	new_test_ext().execute_with(|| {
		let ccm = CcmIngressMetadata {
			message: vec![0x00],
			gas_budget: 1_000,
			refund_address: ForeignChainAddress::Dot(Default::default()),
		};

		assert_noop!(
			Swapping::register_swap_intent(
				RuntimeOrigin::signed(ALICE),
				Asset::Btc,
				Asset::Eth,
				ForeignChainAddress::Dot(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);
		assert_noop!(
			Swapping::ccm_ingress(
				RuntimeOrigin::root(),
				Asset::Btc,
				1_000_000,
				Asset::Eth,
				ForeignChainAddress::Dot(Default::default()),
				ccm.clone()
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_noop!(
			Swapping::register_swap_intent(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Dot,
				ForeignChainAddress::Dot(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
		assert_noop!(
			Swapping::on_ccm_ingress(
				Asset::Eth,
				1_000_000,
				Asset::Dot,
				ForeignChainAddress::Dot(Default::default()),
				ccm.clone()
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
		assert_noop!(
			Swapping::register_swap_intent(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Btc,
				ForeignChainAddress::Btc(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
		assert_noop!(
			Swapping::on_ccm_ingress(
				Asset::Eth,
				1_000_000,
				Asset::Btc,
				ForeignChainAddress::Btc(Default::default()),
				ccm.clone()
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);

		assert_noop!(
			Swapping::on_ccm_ingress(
				Asset::Eth,
				1_000,
				Asset::Eth,
				ForeignChainAddress::Eth(Default::default()),
				ccm
			),
			Error::<Test>::CcmInsufficientIngressAmount
		);
	});
}
