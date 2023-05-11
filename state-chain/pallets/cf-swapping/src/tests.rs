use crate::{
	mock::*, CcmGasBudget, CcmIdCounter, CcmOutputs, CcmSwap, CcmSwapOutput, EarnedBrokerFees,
	Error, Pallet, PendingCcms, Swap, SwapChannelExpiries, SwapQueue, SwapTTL, SwapType,
	WeightInfo,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress, ForeignChainAddress},
	AnyChain, CcmDepositMetadata,
};
use cf_primitives::{Asset, ForeignChain};
use cf_test_utilities::{assert_event_sequence, assert_events_match};
use cf_traits::{
	mocks::{
		address_converter::MockAddressConverter,
		deposit_handler::{MockDepositHandler, SwapChannel},
		egress_handler::{MockEgressHandler, MockEgressParameter},
	},
	CcmHandler, SwapDepositHandler,
};
use frame_support::{assert_noop, assert_ok, sp_std::iter, weights::Weight};

use frame_support::traits::Hooks;
use sp_runtime::traits::BlockNumberProvider;

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
	for (broker_id, swap) in swaps.iter().enumerate() {
		if let SwapType::Swap(destination_address) = &swap.swap_type {
			<Pallet<Test> as SwapDepositHandler>::on_swap_deposit(
				ForeignChainAddress::Eth([2; 20]),
				swap.from,
				swap.to,
				swap.amount,
				destination_address.clone(),
				broker_id as u64,
				2,
			);
		}
	}
}

#[test]
fn request_swap_success_with_valid_parameters() {
	new_test_ext().execute_with(|| {
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
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
				destination_address: if let SwapType::Swap(destination_address) = swap.swap_type {
					destination_address
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
		<Pallet<Test> as SwapDepositHandler>::on_swap_deposit(
			ForeignChainAddress::Eth([2; 20]),
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			200,
		);
		Swapping::on_idle(1, Weight::from_ref_time(1000));
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 2);
		<Pallet<Test> as SwapDepositHandler>::on_swap_deposit(
			ForeignChainAddress::Eth([2; 20]),
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20]),
			ALICE,
			200,
		);
		Swapping::on_idle(1, Weight::from_ref_time(1000));
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 4);
	});
}

#[test]
#[should_panic]
fn cannot_swap_with_incorrect_destination_address_type() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 1_u64;
		<Pallet<Test> as SwapDepositHandler>::on_swap_deposit(
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
		// 1. Request a deposit address -> SwapDepositAddressReady
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			0,
			None
		));
		// 2. Schedule the swap -> SwapDepositReceived
		<Pallet<Test> as SwapDepositHandler>::on_swap_deposit(
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
			crate::mock::RuntimeEvent::Swapping(crate::Event::SwapDepositAddressReady {
				deposit_address: EncodedAddress::Eth(Default::default()),
				destination_address: EncodedAddress::Eth(Default::default()),
				expiry_block: SwapTTL::<Test>::get() + System::current_block_number(),
			}),
			crate::mock::RuntimeEvent::Swapping(crate::Event::SwapDepositReceived {
				deposit_address: EncodedAddress::Eth(Default::default()),
				swap_id: 1,
				deposit_amount: 500,
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
fn withdraw_broker_fees() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			<Error<Test>>::NoFundsAvailable
		);
		EarnedBrokerFees::<Test>::insert(ALICE, Asset::Eth, 200);
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		let mut egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
		assert!(egresses.len() == 1);
		assert_eq!(egresses.pop().expect("must exist").amount(), 200);
		System::assert_last_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::WithdrawalRequested {
				egress_id: (ForeignChain::Ethereum, 1),
				amount: 200,
				address: EncodedAddress::Eth(Default::default()),
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
			EncodedAddress::Eth(Default::default()),
		));

		System::assert_last_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::SwapScheduledByWitnesser {
				swap_id: 1,
				deposit_amount: 1_000,
				destination_address: EncodedAddress::Eth(Default::default()),
			},
		));
	});
}

#[test]
fn swap_expires() {
	new_test_ext().execute_with(|| {
		let expiry = SwapTTL::<Test>::get() + 1;
		assert_eq!(expiry, 6); // Expiry = current(1) + TTL (5)
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			0,
			None
		));

		let deposit_address = assert_events_match!(Test, RuntimeEvent::Swapping(crate::Event::SwapDepositAddressReady {
			deposit_address,
			..
		}) => deposit_address);
		let swap_channel = SwapChannel {
			deposit_address: MockAddressConverter::try_from_encoded_address(deposit_address).unwrap(),
			source_asset: Asset::Eth,
			destination_asset: Asset::Usdc,
			destination_address: ForeignChainAddress::Eth(Default::default()),
			broker_commission_bps: 0,
			broker_id: ALICE,
			message_metadata: None,
		};

		assert_eq!(
			SwapChannelExpiries::<Test>::get(expiry),
			vec![(0, ForeignChainAddress::Eth(Default::default()))]
		);
		assert_eq!(
			MockDepositHandler::<AnyChain, Test>::get_swap_channels(),
			vec![swap_channel.clone()]
		);

		// Does not expire until expiry block.
		Swapping::on_initialize(expiry - 1);
		assert_eq!(
			SwapChannelExpiries::<Test>::get(expiry),
			vec![(0, ForeignChainAddress::Eth(Default::default()))]
		);
		assert_eq!(
			MockDepositHandler::<AnyChain, Test>::get_swap_channels(),
			vec![swap_channel]
		);

		Swapping::on_initialize(6);
		assert_eq!(SwapChannelExpiries::<Test>::get(6), vec![]);
		System::assert_last_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::SwapDepositAddressExpired {
				deposit_address: ForeignChainAddress::Eth(Default::default()),
			},
		));
		assert!(
			MockDepositHandler::<AnyChain, Test>::get_swap_channels().is_empty()
		);
	});
}

#[test]
fn can_set_swap_ttl() {
	new_test_ext().execute_with(|| {
		assert_eq!(crate::SwapTTL::<Test>::get(), 5);
		assert_ok!(Swapping::set_swap_ttl(RuntimeOrigin::root(), 10));
		assert_eq!(crate::SwapTTL::<Test>::get(), 10);
	});
}

#[test]
fn can_reject_invalid_ccms() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let ccm = CcmDepositMetadata {
			message: vec![0x00],
			gas_budget,
			refund_address: ForeignChainAddress::Dot(Default::default()),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};

		assert_noop!(
			Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				Asset::Btc,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);
		assert_noop!(
			Swapping::ccm_deposit(
				RuntimeOrigin::root(),
				Asset::Btc,
				1_000_000,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				ccm.clone()
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_noop!(
			Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Dot,
				EncodedAddress::Dot(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
		assert_noop!(
			Swapping::on_ccm_deposit(
				Asset::Eth,
				1_000_000,
				Asset::Dot,
				ForeignChainAddress::Dot(Default::default()),
				ccm.clone()
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
		assert_noop!(
			Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Btc,
				EncodedAddress::Btc(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
		assert_noop!(
			Swapping::on_ccm_deposit(
				Asset::Eth,
				1_000_000,
				Asset::Btc,
				ForeignChainAddress::Btc(Default::default()),
				ccm.clone()
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);

		assert_noop!(
			Swapping::on_ccm_deposit(
				Asset::Eth,
				gas_budget,
				Asset::Eth,
				ForeignChainAddress::Eth(Default::default()),
				ccm
			),
			Error::<Test>::CcmInsufficientDepositAmount
		);
	});
}

#[test]
fn can_process_ccms_via_swap_deposit_address() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let deposit_amount = 10_000;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			refund_address: ForeignChainAddress::Dot([0x01; 32]),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};

		// Can process CCM via Swap deposit
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			Asset::Dot,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
			0,
			Some(ccm.clone())
		),);
		assert_ok!(Swapping::on_ccm_deposit(
			Asset::Dot,
			deposit_amount,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Dot,
				deposit_amount,
				destination_asset: Asset::Eth,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message_metadata: ccm,
			})
		);

		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![
				Swap {
					swap_id: 1,
					from: Asset::Dot,
					to: Asset::Eth,
					amount: deposit_amount - gas_budget,
					swap_type: SwapType::CcmPrincipal(1)
				},
				Swap {
					swap_id: 2,
					from: Asset::Dot,
					to: Asset::Eth,
					amount: gas_budget,
					swap_type: SwapType::CcmGas(1)
				},
			]
		);

		assert_eq!(CcmOutputs::<Test>::get(1), Some(CcmSwapOutput { principal: None, gas: None }));

		// Swaps are executed during on_idle
		Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Eth,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x01],
				refund_address: ForeignChainAddress::Dot([0x01; 32]),
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: CcmIdCounter::<Test>::get(),
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));
	});
}

#[test]
fn can_process_ccms_via_extrinsic() {
	new_test_ext().execute_with(|| {
		let gas_budget = 2_000;
		let deposit_amount = 1_000_000;
		let ccm = CcmDepositMetadata {
			message: vec![0x02],
			gas_budget,
			refund_address: ForeignChainAddress::Dot([0x02; 32]),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};

		// Can process CCM directly via Pallet Extrinsic.
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Btc,
			deposit_amount,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			ccm.clone()
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Btc,
				deposit_amount,
				destination_asset: Asset::Usdc,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message_metadata: ccm,
			})
		);

		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![
				Swap {
					swap_id: 1,
					from: Asset::Btc,
					to: Asset::Usdc,
					amount: deposit_amount - gas_budget,
					swap_type: SwapType::CcmPrincipal(1)
				},
				Swap {
					swap_id: 2,
					from: Asset::Btc,
					to: Asset::Eth,
					amount: gas_budget,
					swap_type: SwapType::CcmGas(1)
				}
			]
		);
		assert_eq!(CcmOutputs::<Test>::get(1), Some(CcmSwapOutput { principal: None, gas: None }));

		// Swaps are executed during on_idle
		Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x02],
				refund_address: ForeignChainAddress::Dot([0x02; 32]),
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmDepositReceived {
				ccm_id: CcmIdCounter::<Test>::get(),
				principal_swap_id: Some(1),
				gas_swap_id: Some(2),
				deposit_amount,
				destination_address: ForeignChainAddress::Eth(Default::default()),
			},
		));
		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: CcmIdCounter::<Test>::get(),
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));
	});
}

#[test]
fn can_handle_ccms_with_non_native_gas_asset() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let deposit_amount = 10_000;
		let ccm = CcmDepositMetadata {
			message: vec![0x00],
			gas_budget,
			refund_address: ForeignChainAddress::Eth([0x01; 20]),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Eth,
			deposit_amount,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			ccm.clone()
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Eth,
				deposit_amount,
				destination_asset: Asset::Usdc,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message_metadata: ccm,
			})
		);

		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![Swap {
				swap_id: 1,
				from: Asset::Eth,
				to: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				swap_type: SwapType::CcmPrincipal(1)
			},]
		);
		assert_eq!(
			CcmOutputs::<Test>::get(1),
			Some(CcmSwapOutput { principal: None, gas: Some(gas_budget) })
		);

		// Swaps are executed during on_idle
		Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x00],
				refund_address: ForeignChainAddress::Eth([0x01; 20]),
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmDepositReceived {
				ccm_id: CcmIdCounter::<Test>::get(),
				principal_swap_id: Some(1),
				gas_swap_id: None,
				deposit_amount,
				destination_address: ForeignChainAddress::Eth(Default::default()),
			},
		));
		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: CcmIdCounter::<Test>::get(),
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));
	});
}

#[test]
fn can_handle_ccms_with_native_gas_asset() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let deposit_amount = 10_000;
		let ccm = CcmDepositMetadata {
			message: vec![0x00],
			gas_budget,
			refund_address: ForeignChainAddress::Eth([0x01; 20]),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};

		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Usdc,
			deposit_amount,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			ccm.clone()
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Usdc,
				deposit_amount,
				destination_asset: Asset::Usdc,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message_metadata: ccm,
			})
		);

		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![Swap {
				swap_id: 1,
				from: Asset::Usdc,
				to: Asset::Eth,
				amount: gas_budget,
				swap_type: SwapType::CcmGas(1)
			},]
		);
		assert_eq!(
			CcmOutputs::<Test>::get(1),
			Some(CcmSwapOutput { principal: Some(deposit_amount - gas_budget), gas: None })
		);

		// Swaps are executed during on_idle
		Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x00],
				refund_address: ForeignChainAddress::Eth([0x01; 20]),
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmDepositReceived {
				ccm_id: CcmIdCounter::<Test>::get(),
				principal_swap_id: None,
				gas_swap_id: Some(1),
				deposit_amount,
				destination_address: ForeignChainAddress::Eth(Default::default()),
			},
		));
		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: CcmIdCounter::<Test>::get(),
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));
	});
}

#[test]
fn can_handle_ccms_with_no_swaps_needed() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let deposit_amount = 10_000;
		let ccm = CcmDepositMetadata {
			message: vec![0x00],
			gas_budget,
			refund_address: ForeignChainAddress::Eth([0x01; 20]),
			source_address: ForeignChainAddress::Eth([0xcf; 20]),
		};

		// Ccm without need for swapping are egressed directly.
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Eth,
			deposit_amount,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
			ccm
		));

		assert_eq!(PendingCcms::<Test>::get(1), None);

		// The ccm is never put in storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Eth,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x00],
				refund_address: ForeignChainAddress::Eth([0x01; 20]),
			},]
		);

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmEgressScheduled {
				ccm_id: CcmIdCounter::<Test>::get(),
				egress_id: (ForeignChain::Ethereum, 1),
			},
		));

		System::assert_has_event(RuntimeEvent::Swapping(
			crate::Event::<Test>::CcmDepositReceived {
				ccm_id: CcmIdCounter::<Test>::get(),
				principal_swap_id: None,
				gas_swap_id: None,
				deposit_amount,
				destination_address: ForeignChainAddress::Eth(Default::default()),
			},
		));
	});
}
