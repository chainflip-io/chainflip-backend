use crate::{
	mock::{RuntimeEvent, *},
	CcmFailReason, CcmGasBudget, CcmIdCounter, CcmOutputs, CcmSwap, CcmSwapOutput,
	CollectedRejectedFunds, EarnedBrokerFees, Error, Event, MinimumCcmGasBudget, MinimumSwapAmount,
	Pallet, PendingCcms, Swap, SwapChannelExpiries, SwapQueue, SwapTTL, SwapType, WeightInfo,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress, ForeignChainAddress},
	AnyChain, CcmDepositMetadata,
};
use cf_primitives::{Asset, AssetAmount, ForeignChain};
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
		Swap::new(
			1,
			Asset::Flip,
			Asset::Usdc,
			100,
			SwapType::Swap(ForeignChainAddress::Eth([2; 20])),
		),
		// USDC -> asset
		Swap::new(
			2,
			Asset::Eth,
			Asset::Usdc,
			40,
			SwapType::Swap(ForeignChainAddress::Eth([9; 20])),
		),
		// Both assets are on the Eth chain
		Swap::new(
			3,
			Asset::Flip,
			Asset::Eth,
			500,
			SwapType::Swap(ForeignChainAddress::Eth([2; 20])),
		),
		// Cross chain
		Swap::new(
			4,
			Asset::Flip,
			Asset::Dot,
			600,
			SwapType::Swap(ForeignChainAddress::Dot([4; 32])),
		),
	]
}

fn assert_failed_ccm(
	from: Asset,
	amount: AssetAmount,
	output: Asset,
	destination_address: ForeignChainAddress,
	ccm: CcmDepositMetadata,
	reason: CcmFailReason,
) {
	Swapping::on_ccm_deposit(from, amount, output, destination_address.clone(), ccm.clone());
	System::assert_last_event(RuntimeEvent::Swapping(Event::CcmFailed {
		reason,
		destination_address,
		message_metadata: ccm,
	}));
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
			<() as WeightInfo>::execute_group_of_swaps(swaps.len() as u32) *
				(swaps.len() as u64) *
				2,
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
		assert_eq!(SwapQueue::<Test>::get().len(), swaps.len());
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
		// 2. Schedule the swap -> SwapScheduledByDeposit
		<Pallet<Test> as SwapDepositHandler>::on_swap_deposit(
			ForeignChainAddress::Eth(Default::default()),
			Asset::Eth,
			Asset::Usdc,
			500,
			ForeignChainAddress::Eth(Default::default()),
			ALICE,
			0,
		);
		// 3. Process swaps -> SwapCompleted, SwapEgressScheduled
		Swapping::on_idle(1, Weight::from_ref_time(100));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
				deposit_address: EncodedAddress::Eth(Default::default()),
				destination_address: EncodedAddress::Eth(Default::default()),
				expiry_block: SwapTTL::<Test>::get() + System::current_block_number(),
			}),
			RuntimeEvent::Swapping(Event::SwapScheduledByDeposit {
				deposit_address: EncodedAddress::Eth(Default::default()),
				swap_id: 1,
				deposit_amount: 500,
			}),
			RuntimeEvent::Swapping(Event::SwapCompleted { swap_id: 1 }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
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
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::WithdrawalRequested {
			egress_id: (ForeignChain::Ethereum, 1),
			amount: 200,
			address: EncodedAddress::Eth(Default::default()),
		}));
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
			Default::default(),
		));

		System::assert_last_event(RuntimeEvent::Swapping(
			Event::<Test>::SwapScheduledByWitnesser {
				swap_id: 1,
				deposit_amount: 1_000,
				destination_address: EncodedAddress::Eth(Default::default()),
				tx_hash: Default::default(),
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

		let deposit_address = assert_events_match!(Test, RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
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
			Event::<Test>::SwapDepositAddressExpired {
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
			cf_parameters: vec![],
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
		assert_failed_ccm(
			Asset::Eth,
			1_000_000,
			Asset::Dot,
			ForeignChainAddress::Dot(Default::default()),
			ccm.clone(),
			CcmFailReason::UnsupportedForTargetChain,
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

		assert_failed_ccm(
			Asset::Eth,
			1_000_000,
			Asset::Btc,
			ForeignChainAddress::Btc(Default::default()),
			ccm.clone(),
			CcmFailReason::UnsupportedForTargetChain,
		);
		assert_failed_ccm(
			Asset::Eth,
			gas_budget - 1,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm,
			CcmFailReason::InsufficientDepositAmount,
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
			cf_parameters: vec![],
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
		Swapping::on_ccm_deposit(
			Asset::Dot,
			deposit_amount,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
		);

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
				Swap::new(
					1,
					Asset::Dot,
					Asset::Eth,
					deposit_amount - gas_budget,
					SwapType::CcmPrincipal(1)
				),
				Swap::new(2, Asset::Dot, Asset::Eth, gas_budget, SwapType::CcmGas(1)),
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
				cf_parameters: vec![],
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmEgressScheduled {
			ccm_id: CcmIdCounter::<Test>::get(),
			egress_id: (ForeignChain::Ethereum, 1),
		}));
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
			cf_parameters: vec![],
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
				Swap::new(
					1,
					Asset::Btc,
					Asset::Usdc,
					deposit_amount - gas_budget,
					SwapType::CcmPrincipal(1)
				),
				Swap::new(2, Asset::Btc, Asset::Eth, gas_budget, SwapType::CcmGas(1))
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
				cf_parameters: vec![],
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: Some(1),
			gas_swap_id: Some(2),
			deposit_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmEgressScheduled {
			ccm_id: CcmIdCounter::<Test>::get(),
			egress_id: (ForeignChain::Ethereum, 1),
		}));
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
			cf_parameters: vec![],
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
			vec![Swap::new(
				1,
				Asset::Eth,
				Asset::Usdc,
				deposit_amount - gas_budget,
				SwapType::CcmPrincipal(1)
			)]
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
				cf_parameters: vec![],
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: Some(1),
			gas_swap_id: None,
			deposit_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmEgressScheduled {
			ccm_id: CcmIdCounter::<Test>::get(),
			egress_id: (ForeignChain::Ethereum, 1),
		}));
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
			cf_parameters: vec![],
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
			vec![Swap::new(1, Asset::Usdc, Asset::Eth, gas_budget, SwapType::CcmGas(1))]
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
				cf_parameters: vec![],
			},]
		);

		// Gas budgets are stored
		assert_eq!(CcmGasBudget::<Test>::get(1), Some((Asset::Eth, gas_budget)));

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: None,
			gas_swap_id: Some(1),
			deposit_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmEgressScheduled {
			ccm_id: CcmIdCounter::<Test>::get(),
			egress_id: (ForeignChain::Ethereum, 1),
		}));
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
			cf_parameters: vec![],
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
				cf_parameters: vec![],
			},]
		);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmEgressScheduled {
			ccm_id: CcmIdCounter::<Test>::get(),
			egress_id: (ForeignChain::Ethereum, 1),
		}));

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: None,
			gas_swap_id: None,
			deposit_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
	});
}

#[test]
fn can_set_minimum_swap_amount() {
	new_test_ext().execute_with(|| {
		let asset = Asset::Eth;
		let amount = 1_000u128;
		assert_eq!(MinimumSwapAmount::<Test>::get(asset), 0);

		// Set the new minimum swap_amount
		assert_ok!(Swapping::set_minimum_swap_amount(RuntimeOrigin::root(), asset, amount));

		assert_eq!(MinimumSwapAmount::<Test>::get(asset), amount);

		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::MinimumSwapAmountSet {
			asset,
			amount,
		}));
	});
}

#[test]
fn can_set_minimum_ccm_gas_budget() {
	new_test_ext().execute_with(|| {
		let asset = Asset::Eth;
		let amount = 1_000u128;
		assert_eq!(MinimumCcmGasBudget::<Test>::get(asset), 0);

		// Set the new minimum ccm gas budget
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(RuntimeOrigin::root(), asset, amount));

		assert_eq!(MinimumCcmGasBudget::<Test>::get(asset), amount);

		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::MinimumCcmGasBudgetSet {
			asset,
			amount,
		}));
	});
}

#[test]
fn swap_by_witnesser_can_be_rejected() {
	new_test_ext().execute_with(|| {
		let from = Asset::Eth;
		let to = Asset::Flip;
		let amount = 1_000u128;

		// Set minimum swap amount to > deposit amount
		assert_ok!(Swapping::set_minimum_swap_amount(RuntimeOrigin::root(), from, amount + 1));
		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		// Verify this swap is rejected
		assert_eq!(SwapQueue::<Test>::decode_len(), None);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountTooLow {
			asset: from,
			amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		// Fund is confiscated
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), amount);

		// Set minimum swap amount deposit amount
		assert_ok!(Swapping::set_minimum_swap_amount(RuntimeOrigin::root(), from, amount));
		CollectedRejectedFunds::<Test>::set(from, 0);

		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		// Verify this swap is accepted and scheduled
		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![Swap::new(
				1,
				from,
				to,
				amount,
				SwapType::Swap(ForeignChainAddress::Eth(Default::default()),),
			)]
		);
		System::assert_last_event(RuntimeEvent::Swapping(
			Event::<Test>::SwapScheduledByWitnesser {
				swap_id: 1,
				deposit_amount: amount,
				destination_address: EncodedAddress::Eth(Default::default()),
				tx_hash: Default::default(),
			},
		));

		// Confiscated fund is unchanged
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn swap_by_deposit_can_be_rejected() {
	new_test_ext().execute_with(|| {
		let from = Asset::Eth;
		let to = Asset::Flip;
		let amount = 1_000u128;

		// Set minimum swap amount to > deposit amount
		assert_ok!(Swapping::set_minimum_swap_amount(RuntimeOrigin::root(), from, amount + 1));

		Swapping::on_swap_deposit(
			ForeignChainAddress::Eth(Default::default()),
			from,
			to,
			amount,
			ForeignChainAddress::Eth(Default::default()),
			Default::default(),
			Default::default(),
		);

		// Verify this swap is rejected
		assert_eq!(SwapQueue::<Test>::decode_len(), None);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountTooLow {
			asset: from,
			amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		// Fund is confiscated
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), amount);

		// Set minimum swap amount deposit amount
		assert_ok!(Swapping::set_minimum_swap_amount(RuntimeOrigin::root(), from, amount));
		CollectedRejectedFunds::<Test>::set(from, 0);

		Swapping::on_swap_deposit(
			ForeignChainAddress::Eth(Default::default()),
			from,
			to,
			amount,
			ForeignChainAddress::Eth(Default::default()),
			Default::default(),
			Default::default(),
		);

		// Verify this swap is accepted and scheduled
		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![Swap::new(
				1,
				from,
				to,
				amount,
				SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
			)]
		);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduledByDeposit {
			swap_id: 1,
			deposit_address: EncodedAddress::Eth(Default::default()),
			deposit_amount: amount,
		}));

		// Confiscated fund is unchanged
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn cannot_register_ccm_deposit_below_minimum_gas_budget() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let from: Asset = Asset::Eth;
		let to: Asset = Asset::Flip;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			cf_parameters: vec![],
			source_address: ForeignChainAddress::Eth(Default::default()),
		};

		// Set minimum gas budget to be above gas amount
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(
			RuntimeOrigin::root(),
			from,
			gas_budget + 1
		));

		// Register CCM via Swap deposit
		assert_noop!(
			Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				from,
				to,
				EncodedAddress::Eth(Default::default()),
				0,
				Some(ccm.clone())
			),
			Error::<Test>::CcmGasBudgetBelowMinimum
		);

		// Lower the minimum gas budget.
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(RuntimeOrigin::root(), from, gas_budget));
		CollectedRejectedFunds::<Test>::set(from, 0);

		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			EncodedAddress::Eth(Default::default()),
			0,
			Some(ccm)
		));

		// Verify the CCM is reigstered
		assert_eq!(System::current_block_number() + SwapTTL::<Test>::get(), 6);
		assert_events_match!(
			Test,
			RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
				expiry_block: 6,
				..
			}) => ()
		);
	});
}

#[test]
fn ccm_via_exintrincs_below_minimum_gas_budget_are_rejected() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let deposit_amount = 10_000;
		let from: Asset = Asset::Eth;
		let to: Asset = Asset::Flip;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			cf_parameters: vec![],
			source_address: ForeignChainAddress::Eth(Default::default()),
		};

		// Set minimum gas budget to be above gas amount
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(
			RuntimeOrigin::root(),
			from,
			gas_budget + 1
		));

		// Process CCM via extrinsics
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			deposit_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm.clone(),
		));

		// Verify the CCM failed
		assert_eq!(SwapQueue::<Test>::decode_len(), None);
		System::assert_last_event(RuntimeEvent::Swapping(Event::CcmFailed {
			reason: CcmFailReason::GasBudgetBelowMinimum,
			destination_address: ForeignChainAddress::Eth(Default::default()),
			message_metadata: ccm.clone(),
		}));
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), deposit_amount);

		// Lower the minimum gas budget.
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(RuntimeOrigin::root(), from, gas_budget));
		CollectedRejectedFunds::<Test>::set(from, 0);

		// Process CCM via extrinsics
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			deposit_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
		));

		// Verify the CCM succeeded
		assert_eq!(SwapQueue::<Test>::decode_len(), Some(1));
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: 1,
			principal_swap_id: Some(1),
			gas_swap_id: None,
			deposit_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));

		// The funds are not confiscated.
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn ccm_via_deposit_with_principal_below_minimum_are_rejected() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let principal_amount = 2_000;
		let from: Asset = Asset::Eth;
		let to: Asset = Asset::Flip;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			cf_parameters: vec![],
			source_address: ForeignChainAddress::Eth(Default::default()),
		};

		// Set minimum gas budget to be above gas amount
		assert_ok!(Swapping::set_minimum_swap_amount(
			RuntimeOrigin::root(),
			from,
			principal_amount + 1
		));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			EncodedAddress::Eth(Default::default()),
			0,
			Some(ccm.clone())
		));

		assert_failed_ccm(
			from,
			gas_budget + principal_amount,
			to,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
			CcmFailReason::PrincipalSwapAmountTooLow,
		);

		// Verify the ccm is rejected
		assert_eq!(SwapQueue::<Test>::decode_len(), None);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), gas_budget + principal_amount);

		// Lower the minimum gas budget.
		assert_ok!(Swapping::set_minimum_swap_amount(
			RuntimeOrigin::root(),
			from,
			principal_amount
		));
		CollectedRejectedFunds::<Test>::set(from, 0);

		Swapping::on_ccm_deposit(
			from,
			gas_budget + principal_amount,
			to,
			ForeignChainAddress::Eth(Default::default()),
			ccm,
		);

		// Verify the CCM is processed successfully
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: 1,
			principal_swap_id: Some(1),
			gas_swap_id: None,
			deposit_amount: gas_budget + principal_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		assert_eq!(SwapQueue::<Test>::decode_len(), Some(1));
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn ccm_via_extrinsic_with_principal_below_minimum_are_rejected() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let principal_amount = 2_000;
		let from: Asset = Asset::Eth;
		let to: Asset = Asset::Flip;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			cf_parameters: vec![],
			source_address: ForeignChainAddress::Eth(Default::default()),
		};

		// Set minimum gas budget to be above gas amount
		assert_ok!(Swapping::set_minimum_swap_amount(
			RuntimeOrigin::root(),
			from,
			principal_amount + 1
		));

		// Register CCM via extrinsic
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			gas_budget + principal_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm.clone(),
		));

		// Verify the ccm is rejected
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::CcmFailed {
			reason: CcmFailReason::PrincipalSwapAmountTooLow,
			destination_address: ForeignChainAddress::Eth(Default::default()),
			message_metadata: ccm.clone(),
		}));
		assert_eq!(SwapQueue::<Test>::decode_len(), None);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), gas_budget + principal_amount);

		// Lower the minimum gas budget.
		assert_ok!(Swapping::set_minimum_swap_amount(
			RuntimeOrigin::root(),
			from,
			principal_amount
		));
		CollectedRejectedFunds::<Test>::set(from, 0);

		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			gas_budget + principal_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
		));

		// Verify the CCM is processed successfully
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: 1,
			principal_swap_id: Some(1),
			gas_swap_id: None,
			deposit_amount: gas_budget + principal_amount,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		assert_eq!(SwapQueue::<Test>::decode_len(), Some(1));
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn ccm_without_principal_swaps_are_accepted() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let principal_amount = 10_000;
		let eth: Asset = Asset::Eth;
		let flip: Asset = Asset::Flip;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			cf_parameters: vec![],
			source_address: ForeignChainAddress::Eth(Default::default()),
		};

		// Set minimum swap and gas budget.
		assert_ok!(Swapping::set_minimum_swap_amount(
			RuntimeOrigin::root(),
			eth,
			principal_amount + 1,
		));
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(RuntimeOrigin::root(), eth, gas_budget));
		assert_ok!(Swapping::set_minimum_swap_amount(
			RuntimeOrigin::root(),
			flip,
			principal_amount + 1,
		));
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(RuntimeOrigin::root(), flip, gas_budget));
		System::reset_events();

		// Ccm with principal asset = 0
		Swapping::on_ccm_deposit(
			eth,
			gas_budget,
			flip,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
		);

		// Verify the CCM is processed successfully
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 1,
				principal_swap_id: None,
				gas_swap_id: None,
				deposit_amount: gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
			}),
			RuntimeEvent::Swapping(Event::CcmEgressScheduled {
				ccm_id: 1,
				egress_id: (ForeignChain::Ethereum, 1),
			})
		);
		// No funds are confiscated
		assert_eq!(CollectedRejectedFunds::<Test>::get(eth), 0);
		assert_eq!(CollectedRejectedFunds::<Test>::get(flip), 0);

		// Ccm where principal asset = output asset
		System::reset_events();
		Swapping::on_ccm_deposit(
			eth,
			gas_budget + principal_amount,
			eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm,
		);

		// Verify the CCM is processed successfully
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 2,
				principal_swap_id: None,
				gas_swap_id: None,
				deposit_amount: gas_budget + principal_amount,
				destination_address: ForeignChainAddress::Eth(Default::default()),
			}),
			RuntimeEvent::Swapping(Event::CcmEgressScheduled {
				ccm_id: 2,
				egress_id: (ForeignChain::Ethereum, 2),
			})
		);
		// No funds are confiscated
		assert_eq!(CollectedRejectedFunds::<Test>::get(eth), 0);
		assert_eq!(CollectedRejectedFunds::<Test>::get(flip), 0);
	});
}

#[test]
fn ccm_with_gas_below_minimum_swap_amount_allowed() {
	new_test_ext().execute_with(|| {
		let gas_budget = 1_000;
		let flip: Asset = Asset::Flip;
		let ccm = CcmDepositMetadata {
			message: vec![0x01],
			gas_budget,
			cf_parameters: vec![],
			source_address: ForeignChainAddress::Eth(Default::default()),
		};

		// Set minimum swap and gas budget.
		assert_ok!(Swapping::set_minimum_swap_amount(RuntimeOrigin::root(), flip, gas_budget + 1,));
		assert_ok!(Swapping::set_minimum_ccm_gas_budget(RuntimeOrigin::root(), flip, gas_budget));
		System::reset_events();

		// Even if gas amount is below minimum swap amount, it is allowed.
		Swapping::on_ccm_deposit(
			flip,
			gas_budget,
			flip,
			ForeignChainAddress::Eth(Default::default()),
			ccm,
		);

		// Verify the CCM is processed successfully
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: 1,
			principal_swap_id: None,
			gas_swap_id: Some(1),
			deposit_amount: gas_budget,
			destination_address: ForeignChainAddress::Eth(Default::default()),
		}));
		// No funds are confiscated
		assert_eq!(CollectedRejectedFunds::<Test>::get(flip), 0);
	});
}

#[test]
fn process_all_into_stable_swaps_first() {
	new_test_ext().execute_with(|| {
		let amount = 1_000_000;
		let encoded_address = EncodedAddress::Eth(Default::default());
		let address = ForeignChainAddress::Eth(Default::default());
		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			Asset::Flip,
			Asset::Eth,
			amount,
			encoded_address.clone(),
			Default::default(),
		));
		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			Asset::Btc,
			Asset::Eth,
			amount,
			encoded_address.clone(),
			Default::default(),
		));
		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			Asset::Dot,
			Asset::Eth,
			amount,
			encoded_address.clone(),
			Default::default(),
		));
		assert_ok!(Swapping::schedule_swap_by_witnesser(
			RuntimeOrigin::root(),
			Asset::Usdc,
			Asset::Eth,
			amount,
			encoded_address,
			Default::default(),
		));
		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![
				Swap {
					swap_id: 1,
					from: Asset::Flip,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: None
				},
				Swap {
					swap_id: 2,
					from: Asset::Btc,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: None
				},
				Swap {
					swap_id: 3,
					from: Asset::Dot,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: None
				},
				Swap {
					swap_id: 4,
					from: Asset::Usdc,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: Some(amount)
				},
			]
		);

		System::reset_events();
		// All swaps -> stable are executed
		Swapping::on_idle(1, MockWeightInfo::execute_group_of_swaps(1) * 3);
		assert_eq!(
			SwapQueue::<Test>::get(),
			vec![
				Swap {
					swap_id: 4,
					from: Asset::Usdc,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: Some(amount)
				},
				Swap {
					swap_id: 1,
					from: Asset::Flip,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: Some(amount)
				},
				Swap {
					swap_id: 3,
					from: Asset::Dot,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address.clone()),
					stable_amount: Some(amount)
				},
				Swap {
					swap_id: 2,
					from: Asset::Btc,
					to: Asset::Eth,
					amount,
					swap_type: SwapType::Swap(address),
					stable_amount: Some(amount)
				},
			]
		);

		// Complete the remaining swaps
		Swapping::on_idle(1, MockWeightInfo::execute_group_of_swaps(4));

		assert!(SwapQueue::<Test>::get().is_empty());
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapCompleted { swap_id: 4 }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 4,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 1),
				amount
			}),
			RuntimeEvent::Swapping(Event::SwapCompleted { swap_id: 1 }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 1,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 2),
				amount
			}),
			RuntimeEvent::Swapping(Event::SwapCompleted { swap_id: 3 }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 3,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 3),
				amount
			}),
			RuntimeEvent::Swapping(Event::SwapCompleted { swap_id: 2 }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 2,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 4),
				amount
			})
		);
	});
}
