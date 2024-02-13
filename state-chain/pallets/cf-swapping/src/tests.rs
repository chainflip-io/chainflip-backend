use super::*;
use crate::{
	mock::{RuntimeEvent, *},
	CcmFailReason, CcmIdCounter, CcmOutputs, CcmSwap, CcmSwapOutput, CollectedRejectedFunds,
	EarnedBrokerFees, Error, Event, MaximumSwapAmount, Pallet, PendingCcms, Swap, SwapOrigin,
	SwapQueue, SwapType,
};
use cf_chains::{
	address::{to_encoded_address, AddressConverter, EncodedAddress, ForeignChainAddress},
	btc::{BitcoinNetwork, ScriptPubkey},
	dot::PolkadotAccountId,
	AnyChain, CcmChannelMetadata, CcmDepositMetadata,
};
use cf_primitives::{Asset, AssetAmount, BasisPoints, ForeignChain, NetworkEnvironment};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::{
		address_converter::MockAddressConverter,
		egress_handler::{MockEgressHandler, MockEgressParameter},
	},
	CcmHandler, SetSafeMode, SwapDepositHandler, SwappingApi,
};
use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Hooks, OriginTrait},
};
use itertools::Itertools;
use sp_arithmetic::Permill;
use sp_std::iter;

const GAS_BUDGET: AssetAmount = 1_000u128;

fn set_maximum_swap_amount(asset: Asset, amount: Option<AssetAmount>) {
	assert_ok!(Swapping::update_pallet_config(
		OriginTrait::root(),
		vec![PalletConfigUpdate::MaximumSwapAmount { asset, amount }]
			.try_into()
			.unwrap()
	));
}

// Returns some test data
fn generate_test_swaps() -> Vec<Swap> {
	vec![
		// asset -> USDC
		Swap::new(
			1,
			Asset::Flip,
			Asset::Usdc,
			100,
			SwapType::Swap(ForeignChainAddress::Eth([2; 20].into())),
		),
		// USDC -> asset
		Swap::new(
			2,
			Asset::Eth,
			Asset::Usdc,
			40,
			SwapType::Swap(ForeignChainAddress::Eth([9; 20].into())),
		),
		// Both assets are on the Eth chain
		Swap::new(
			3,
			Asset::Flip,
			Asset::Eth,
			500,
			SwapType::Swap(ForeignChainAddress::Eth([2; 20].into())),
		),
		// Cross chain
		Swap::new(
			4,
			Asset::Flip,
			Asset::Dot,
			600,
			SwapType::Swap(ForeignChainAddress::Dot(PolkadotAccountId::from_aliased([4; 32]))),
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
	assert_err!(
		Swapping::on_ccm_deposit(
			from,
			amount,
			output,
			destination_address.clone(),
			ccm.clone(),
			SwapOrigin::Vault { tx_hash: Default::default() },
		),
		()
	);
	System::assert_last_event(RuntimeEvent::Swapping(Event::CcmFailed {
		reason,
		destination_address: MockAddressConverter::to_encoded_address(destination_address),
		deposit_metadata: ccm,
	}));
}

fn insert_swaps(swaps: &[Swap]) {
	for (broker_id, swap) in swaps.iter().enumerate() {
		if let SwapType::Swap(destination_address) = &swap.swap_type {
			<Pallet<Test> as SwapDepositHandler>::schedule_swap_from_channel(
				ForeignChainAddress::Eth([2; 20].into()),
				Default::default(),
				swap.from,
				swap.to,
				swap.amount,
				destination_address.clone(),
				broker_id as u64,
				2,
				1,
			);
		}
	}
}

fn generate_ccm_channel() -> CcmChannelMetadata {
	CcmChannelMetadata {
		message: vec![0x01].try_into().unwrap(),
		gas_budget: GAS_BUDGET,
		cf_parameters: Default::default(),
	}
}
fn generate_ccm_deposit() -> CcmDepositMetadata {
	CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
		channel_metadata: generate_ccm_channel(),
	}
}

#[track_caller]
fn assert_swaps_queue_is_empty() {
	assert_eq!(SwapQueue::<Test>::iter_keys().count(), 0);
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
			None,
			0
		));
	});
}

#[test]
fn process_all_swaps() {
	new_test_ext().execute_with(|| {
		let swaps = generate_test_swaps();
		insert_swaps(&swaps);
		Swapping::on_finalize(System::block_number() + SWAP_DELAY_BLOCKS as u64);
		assert_swaps_queue_is_empty();
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
				fee: 0,
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
fn expect_earned_fees_to_be_recorded() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 2_u64;
		<Pallet<Test> as SwapDepositHandler>::schedule_swap_from_channel(
			ForeignChainAddress::Eth([2; 20].into()),
			Default::default(),
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20].into()),
			ALICE,
			200,
			1,
		);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 2);
		<Pallet<Test> as SwapDepositHandler>::schedule_swap_from_channel(
			ForeignChainAddress::Eth([2; 20].into()),
			Default::default(),
			Asset::Flip,
			Asset::Usdc,
			100,
			ForeignChainAddress::Eth([2; 20].into()),
			ALICE,
			200,
			1,
		);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Flip), 4);
	});
}

#[test]
#[should_panic]
fn cannot_swap_with_incorrect_destination_address_type() {
	new_test_ext().execute_with(|| {
		const ALICE: u64 = 1_u64;
		<Pallet<Test> as SwapDepositHandler>::schedule_swap_from_channel(
			ForeignChainAddress::Eth([2; 20].into()),
			Default::default(),
			Asset::Eth,
			Asset::Dot,
			10,
			ForeignChainAddress::Eth([2; 20].into()),
			ALICE,
			2,
			1,
		);

		assert_swaps_queue_is_empty();
	});
}

#[test]
fn expect_swap_id_to_be_emitted() {
	new_test_ext()
		.execute_with(|| {
			// 1. Request a deposit address -> SwapDepositAddressReady
			assert_ok!(Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Usdc,
				EncodedAddress::Eth(Default::default()),
				0,
				None,
				0
			));

			const AMOUNT: AssetAmount = 500;
			// 2. Schedule the swap -> SwapScheduled
			<Pallet<Test> as SwapDepositHandler>::schedule_swap_from_channel(
				ForeignChainAddress::Eth(Default::default()),
				Default::default(),
				Asset::Eth,
				Asset::Usdc,
				AMOUNT,
				ForeignChainAddress::Eth(Default::default()),
				ALICE,
				0,
				1,
			);
			// 3. Process swaps -> SwapExecuted, SwapEgressScheduled
			Swapping::on_finalize(1);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapDepositAddressReady {
					deposit_address: EncodedAddress::Eth(..),
					destination_address: EncodedAddress::Eth(..),
					source_asset: Asset::Eth,
					destination_asset: Asset::Usdc,
					channel_id: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 1,
					source_asset: Asset::Eth,
					deposit_amount: AMOUNT,
					destination_asset: Asset::Usdc,
					destination_address: EncodedAddress::Eth(..),
					origin: SwapOrigin::DepositChannel {
						deposit_address: EncodedAddress::Eth(..),
						channel_id: 1,
						deposit_block_height: 0
					},
					swap_type: SwapType::Swap(ForeignChainAddress::Eth(..)),
					broker_commission: _,
					..
				})
			);
		})
		.then_process_blocks_until(|_| System::block_number() == 3)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_id: 1,
					egress_id: (ForeignChain::Ethereum, 1),
					asset: Asset::Usdc,
					amount: 500,
					fee: _,
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
			egress_amount: 200,
			destination_address: EncodedAddress::Eth(Default::default()),
			egress_fee: 0,
		}));
	});
}

#[test]
fn can_swap_using_witness_origin() {
	new_test_ext().execute_with(|| {
		let from = Asset::Eth;
		let to = Asset::Flip;
		let amount = 1_000;

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
			swap_id: 1,
			source_asset: from,
			deposit_amount: amount,
			destination_asset: to,
			destination_address: EncodedAddress::Eth(Default::default()),
			origin: SwapOrigin::Vault { tx_hash: Default::default() },
			swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
			broker_commission: None,
			execute_at: 3,
		}));
	});
}

#[test]
fn reject_invalid_ccm_deposit() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let ccm = generate_ccm_deposit();

		assert_noop!(
			Swapping::ccm_deposit(
				RuntimeOrigin::root(),
				Asset::Btc,
				1_000_000,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				ccm.clone(),
				Default::default(),
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
				ccm.clone(),
				Default::default(),
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_failed_ccm(
			Asset::Eth,
			1_000_000,
			Asset::Dot,
			ForeignChainAddress::Dot(Default::default()),
			ccm.clone(),
			CcmFailReason::UnsupportedForTargetChain,
		);

		assert_failed_ccm(
			Asset::Eth,
			1_000_000,
			Asset::Btc,
			ForeignChainAddress::Btc(cf_chains::btc::ScriptPubkey::P2PKH(Default::default())),
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
fn rejects_invalid_swap_deposit() {
	new_test_ext().execute_with(|| {
		let ccm = generate_ccm_channel();

		assert_noop!(
			Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				Asset::Btc,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
				0,
				Some(ccm.clone()),
				0
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
				Some(ccm),
				0
			),
			Error::<Test>::CcmUnsupportedForTargetChain
		);
	});
}

#[test]
fn rejects_invalid_swap_by_witnesser() {
	new_test_ext().execute_with(|| {
		let script_pubkey = ScriptPubkey::try_from_address(
			"BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
			&BitcoinNetwork::Mainnet,
		)
		.unwrap();

		let btc_encoded_address =
			to_encoded_address(ForeignChainAddress::Btc(script_pubkey), || {
				NetworkEnvironment::Mainnet
			});

		// Is valid Bitcoin address, but asset is Dot, so not compatible
		assert_noop!(
			Swapping::schedule_swap_from_contract(
				RuntimeOrigin::root(),
				Asset::Eth,
				Asset::Dot,
				10000,
				btc_encoded_address,
				Default::default()
			),
			Error::<Test>::IncompatibleAssetAndAddress
		);

		assert_noop!(
			Swapping::schedule_swap_from_contract(
				RuntimeOrigin::root(),
				Asset::Eth,
				Asset::Btc,
				10000,
				EncodedAddress::Btc(vec![0x41, 0x80, 0x41]),
				Default::default()
			),
			Error::<Test>::InvalidDestinationAddress
		);
	});
}

#[test]
fn can_process_ccms_via_swap_deposit_address() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let deposit_amount = 10_000;
		let request_ccm = generate_ccm_channel();
		let ccm = generate_ccm_deposit();

		// Can process CCM via Swap deposit
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			Asset::Dot,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
			0,
			Some(request_ccm),
			0
		));
		assert_ok!(Swapping::on_ccm_deposit(
			Asset::Dot,
			deposit_amount,
			Asset::Eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
			SwapOrigin::Vault { tx_hash: Default::default() },
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Dot,
				deposit_amount,
				destination_asset: Asset::Eth,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				deposit_metadata: ccm,
				principal_swap_id: Some(1),
				gas_swap_id: Some(2),
			})
		);

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap::new(
					1,
					Asset::Dot,
					Asset::Eth,
					deposit_amount - gas_budget,
					SwapType::CcmPrincipal(1),
				),
				Swap::new(2, Asset::Dot, Asset::Eth, gas_budget, SwapType::CcmGas(1)),
			]
		);

		assert_eq!(CcmOutputs::<Test>::get(1), Some(CcmSwapOutput { principal: None, gas: None }));

		// Swaps are executed during on_finalize after SWAP_DELAY_BLOCKS delay
		Swapping::on_finalize(execute_at);

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Eth,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x01].try_into().unwrap(),
				cf_parameters: vec![].try_into().unwrap(),
				gas_budget,
			},]
		);

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
		let gas_budget = GAS_BUDGET;
		let deposit_amount = 1_000_000;
		let ccm = generate_ccm_deposit();

		// Can process CCM directly via Pallet Extrinsic.
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Btc,
			deposit_amount,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			ccm.clone(),
			Default::default(),
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Btc,
				deposit_amount,
				destination_asset: Asset::Usdc,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				deposit_metadata: ccm.clone(),
				principal_swap_id: Some(1),
				gas_swap_id: Some(2),
			})
		);

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap::new(
					1,
					Asset::Btc,
					Asset::Usdc,
					deposit_amount - gas_budget,
					SwapType::CcmPrincipal(1),
				),
				Swap::new(2, Asset::Btc, Asset::Eth, gas_budget, SwapType::CcmGas(1))
			]
		);
		assert_eq!(CcmOutputs::<Test>::get(1), Some(CcmSwapOutput { principal: None, gas: None }));

		// Swaps are executed during on_finalize
		Swapping::on_finalize(execute_at);

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x01].try_into().unwrap(),
				cf_parameters: vec![].try_into().unwrap(),
				gas_budget,
			},]
		);

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: Some(1),
			gas_swap_id: Some(2),
			deposit_amount,
			destination_address: EncodedAddress::Eth(Default::default()),
			deposit_metadata: ccm,
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
		let gas_budget = GAS_BUDGET;
		let deposit_amount = 10_000;
		let ccm = generate_ccm_deposit();
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Eth,
			deposit_amount,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			ccm.clone(),
			Default::default(),
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Eth,
				deposit_amount,
				destination_asset: Asset::Usdc,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				deposit_metadata: ccm.clone(),
				principal_swap_id: Some(1),
				gas_swap_id: None,
			})
		);

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(
				1,
				Asset::Eth,
				Asset::Usdc,
				deposit_amount - gas_budget,
				SwapType::CcmPrincipal(1),
			)]
		);
		assert_eq!(
			CcmOutputs::<Test>::get(1),
			Some(CcmSwapOutput { principal: None, gas: Some(gas_budget) })
		);

		// Swaps are executed during on_finalize
		Swapping::on_finalize(execute_at);

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x01].try_into().unwrap(),
				cf_parameters: vec![].try_into().unwrap(),
				gas_budget,
			},]
		);

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: Some(1),
			gas_swap_id: None,
			deposit_amount,
			destination_address: EncodedAddress::Eth(Default::default()),
			deposit_metadata: ccm,
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
		let gas_budget = GAS_BUDGET;
		let deposit_amount = 10_000;
		let ccm = generate_ccm_deposit();

		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Usdc,
			deposit_amount,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			ccm.clone(),
			Default::default(),
		));

		assert_eq!(
			PendingCcms::<Test>::get(1),
			Some(CcmSwap {
				source_asset: Asset::Usdc,
				deposit_amount,
				destination_asset: Asset::Usdc,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				deposit_metadata: ccm.clone(),
				principal_swap_id: None,
				gas_swap_id: Some(1),
			})
		);

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(1, Asset::Usdc, Asset::Eth, gas_budget, SwapType::CcmGas(1),)]
		);
		assert_eq!(
			CcmOutputs::<Test>::get(1),
			Some(CcmSwapOutput { principal: Some(deposit_amount - gas_budget), gas: None })
		);

		// Swaps are executed during on_finalize
		Swapping::on_finalize(execute_at);

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Usdc,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x01].try_into().unwrap(),
				cf_parameters: vec![].try_into().unwrap(),
				gas_budget,
			},]
		);

		// Completed CCM is removed from storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::CcmDepositReceived {
			ccm_id: CcmIdCounter::<Test>::get(),
			principal_swap_id: None,
			gas_swap_id: Some(1),
			deposit_amount,
			destination_address: EncodedAddress::Eth(Default::default()),
			deposit_metadata: ccm,
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
		let gas_budget = GAS_BUDGET;
		let deposit_amount = 10_000;
		let ccm = generate_ccm_deposit();

		// Ccm without need for swapping are egressed directly.
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			Asset::Eth,
			deposit_amount,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
			ccm.clone(),
			Default::default(),
		));

		assert_eq!(PendingCcms::<Test>::get(1), None);

		// The ccm is never put in storage
		assert_eq!(PendingCcms::<Test>::get(1), None);
		assert_eq!(CcmOutputs::<Test>::get(1), None);

		// CCM is scheduled for egress
		assert_eq!(
			MockEgressHandler::<AnyChain>::get_scheduled_egresses(),
			vec![MockEgressParameter::Ccm {
				asset: Asset::Eth,
				amount: deposit_amount - gas_budget,
				destination_address: ForeignChainAddress::Eth(Default::default()),
				message: vec![0x01].try_into().unwrap(),
				cf_parameters: vec![].try_into().unwrap(),
				gas_budget,
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
			destination_address: EncodedAddress::Eth(Default::default()),
			deposit_metadata: ccm,
		}));
	});
}

#[test]
fn swap_by_witnesser_happy_path() {
	new_test_ext().execute_with(|| {
		let from = Asset::Eth;
		let to = Asset::Flip;
		let amount = 1_000u128;

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		// Verify this swap is accepted and scheduled
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(
				1,
				from,
				to,
				amount,
				SwapType::Swap(ForeignChainAddress::Eth(Default::default()),),
			)]
		);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
			swap_id: 1,
			source_asset: from,
			deposit_amount: amount,
			destination_asset: to,
			destination_address: EncodedAddress::Eth(Default::default()),
			origin: SwapOrigin::Vault { tx_hash: Default::default() },
			swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
			broker_commission: None,
			execute_at,
		}));

		// Confiscated fund is unchanged
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn swap_by_deposit_happy_path() {
	new_test_ext().execute_with(|| {
		let from = Asset::Eth;
		let to = Asset::Flip;
		let amount = 1_000u128;

		Swapping::schedule_swap_from_channel(
			ForeignChainAddress::Eth(Default::default()),
			Default::default(),
			from,
			to,
			amount,
			ForeignChainAddress::Eth(Default::default()),
			Default::default(),
			Default::default(),
			1,
		);

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		// Verify this swap is accepted and scheduled
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![Swap::new(
				1,
				from,
				to,
				amount,
				SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
			)]
		);
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
			swap_id: 1,
			deposit_amount: amount,
			source_asset: from,
			destination_asset: to,
			destination_address: EncodedAddress::Eth(Default::default()),
			origin: SwapOrigin::DepositChannel {
				deposit_address: EncodedAddress::Eth(Default::default()),
				channel_id: 1,
				deposit_block_height: Default::default(),
			},
			swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
			broker_commission: Some(0),
			execute_at,
		}));

		// Confiscated fund is unchanged
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn ccm_without_principal_swaps_are_accepted() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 10_000;
		let eth: Asset = Asset::Eth;
		let flip: Asset = Asset::Flip;
		let ccm = generate_ccm_deposit();

		// Ccm with principal asset = 0
		assert_ok!(Swapping::on_ccm_deposit(
			eth,
			gas_budget,
			flip,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
			SwapOrigin::Vault { tx_hash: Default::default() },
		));

		// Verify the CCM is processed successfully
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 1,
				principal_swap_id: None,
				gas_swap_id: None,
				deposit_amount,
				destination_address: EncodedAddress::Eth(..),
				deposit_metadata: _,
			}) if deposit_amount == gas_budget,
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
		assert_ok!(Swapping::on_ccm_deposit(
			eth,
			gas_budget + principal_amount,
			eth,
			ForeignChainAddress::Eth(Default::default()),
			ccm,
			SwapOrigin::Vault { tx_hash: Default::default() },
		));

		// Verify the CCM is processed successfully
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 2,
				principal_swap_id: None,
				gas_swap_id: None,
				deposit_amount,
				destination_address: EncodedAddress::Eth(..),
				deposit_metadata: _,
			}) if deposit_amount == gas_budget + principal_amount,
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
fn process_all_into_stable_swaps_first() {
	new_test_ext().execute_with(|| {
		let amount = 1_000_000;
		let encoded_address = EncodedAddress::Eth(Default::default());
		let address = ForeignChainAddress::Eth(Default::default());
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			Asset::Flip,
			Asset::Eth,
			amount,
			encoded_address.clone(),
			Default::default(),
		));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			Asset::Btc,
			Asset::Eth,
			amount,
			encoded_address.clone(),
			Default::default(),
		));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			Asset::Dot,
			Asset::Eth,
			amount,
			encoded_address.clone(),
			Default::default(),
		));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::root(),
			Asset::Usdc,
			Asset::Eth,
			amount,
			encoded_address,
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap::new(1, Asset::Flip, Asset::Eth, amount, SwapType::Swap(address.clone()),),
				Swap::new(2, Asset::Btc, Asset::Eth, amount, SwapType::Swap(address.clone()),),
				Swap::new(3, Asset::Dot, Asset::Eth, amount, SwapType::Swap(address.clone()),),
				Swap::new(4, Asset::Usdc, Asset::Eth, amount, SwapType::Swap(address)),
			]
		);

		System::reset_events();
		// All swaps in the SwapQueue are executed.
		Swapping::on_finalize(execute_at);
		assert_swaps_queue_is_empty();

		// Network fee should only be taken once.
		let total_amount_after_network_fee = MockSwappingApi::take_network_fee(amount * 4);
		let output_amount = total_amount_after_network_fee / 4;
		// Verify swap "from" -> STABLE_ASSET, then "to" -> Output Asset
		assert_eq!(
			Swaps::get(),
			vec![
				(Asset::Flip, Asset::Usdc, amount),
				(Asset::Dot, Asset::Usdc, amount),
				(Asset::Btc, Asset::Usdc, amount),
				(Asset::Usdc, Asset::Eth, total_amount_after_network_fee),
			]
		);

		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 1,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 1),
				amount,
				fee: _,
			}) if amount == output_amount,
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 2,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 2),
				amount,
				fee: _,
			}) if amount == output_amount,
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 3,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 3),
				amount,
				fee: _,
			}) if amount == output_amount,
			RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 4, .. }),
			RuntimeEvent::Swapping(Event::SwapEgressScheduled {
				swap_id: 4,
				asset: Asset::Eth,
				egress_id: (ForeignChain::Ethereum, 4),
				amount,
				fee: _,
			}) if amount == output_amount,
		);
	});
}

#[test]
fn cannot_swap_in_safe_mode() {
	new_test_ext().execute_with(|| {
		let swaps_scheduled_at = System::block_number() + SWAP_DELAY_BLOCKS as u64;

		SwapQueue::<Test>::insert(swaps_scheduled_at, generate_test_swaps());

		assert_eq!(SwapQueue::<Test>::decode_len(swaps_scheduled_at), Some(4));

		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// No swap is done
		Swapping::on_finalize(swaps_scheduled_at);
		assert_eq!(SwapQueue::<Test>::decode_len(swaps_scheduled_at), Some(4));

		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// Swaps are processed
		Swapping::on_finalize(swaps_scheduled_at + 1);
		assert_eq!(SwapQueue::<Test>::decode_len(swaps_scheduled_at), None);
	});
}

#[test]
fn cannot_withdraw_in_safe_mode() {
	new_test_ext().execute_with(|| {
		EarnedBrokerFees::<Test>::insert(ALICE, Asset::Eth, 200);

		// Activate code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// Cannot withdraw
		assert_noop!(
			Swapping::withdraw(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			Error::<Test>::WithdrawalsDisabled
		);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, Asset::Eth), 200);

		// Change back to code green
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// withdraws are now alloed
		assert_ok!(Swapping::withdraw(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, Asset::Eth), 0);
	});
}

#[test]
fn ccm_swaps_emits_events() {
	new_test_ext().execute_with(|| {
		let ccm = generate_ccm_deposit();
		let destination_address = ForeignChainAddress::Eth(Default::default());

		const ORIGIN: SwapOrigin = SwapOrigin::Vault { tx_hash: [0x11; 32] };

		// Test when both principal and gas need to be swapped.
		System::reset_events();
		assert_ok!(Swapping::on_ccm_deposit(
			Asset::Flip,
			10_000,
			Asset::Usdc,
			destination_address.clone(),
			ccm.clone(),
			ORIGIN,
		));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapScheduled {
				swap_id: 1,
				source_asset: Asset::Flip,
				deposit_amount: 9_000,
				destination_asset: Asset::Usdc,
				destination_address: EncodedAddress::Eth(..),
				origin: ORIGIN,
				swap_type: SwapType::CcmPrincipal(1),
				broker_commission: _,
				..
			}),
			RuntimeEvent::Swapping(Event::SwapScheduled {
				swap_type: SwapType::CcmGas(1),
				swap_id: 2,
				source_asset: Asset::Flip,
				deposit_amount: 1_000,
				destination_asset: Asset::Eth,
				destination_address: EncodedAddress::Eth(..),
				origin: ORIGIN,
				..
			}),
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 1,
				principal_swap_id: Some(1),
				gas_swap_id: Some(2),
				deposit_amount: 10_000,
				..
			}),
		);

		// Test when only principal needs to be swapped.
		System::reset_events();
		assert_ok!(Swapping::on_ccm_deposit(
			Asset::Eth,
			10_000,
			Asset::Usdc,
			destination_address.clone(),
			ccm.clone(),
			ORIGIN,
		));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapScheduled {
				swap_type: SwapType::CcmPrincipal(2),
				swap_id: 3,
				source_asset: Asset::Eth,
				deposit_amount: 9_000,
				destination_asset: Asset::Usdc,
				destination_address: EncodedAddress::Eth(..),
				origin: ORIGIN,
				..
			}),
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 2,
				principal_swap_id: Some(3),
				gas_swap_id: None,
				deposit_amount: 10_000,
				..
			}),
		);

		// Test when only gas needs to be swapped.
		System::reset_events();
		assert_ok!(Swapping::on_ccm_deposit(
			Asset::Flip,
			10_000,
			Asset::Flip,
			destination_address,
			ccm,
			ORIGIN,
		));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapScheduled {
				swap_type: SwapType::CcmGas(3),
				swap_id: 4,
				source_asset: Asset::Flip,
				deposit_amount: 1_000,
				destination_asset: Asset::Eth,
				destination_address: EncodedAddress::Eth(..),
				origin: ORIGIN,
				..
			}),
			RuntimeEvent::Swapping(Event::CcmDepositReceived {
				ccm_id: 3,
				principal_swap_id: None,
				gas_swap_id: Some(4),
				deposit_amount: 10_000,
				..
			}),
		);
	});
}

#[test]
fn can_handle_ccm_with_zero_swap_outputs() {
	new_test_ext()
		.then_execute_at_next_block(|_| {
			let eth_address = ForeignChainAddress::Eth(Default::default());
			let ccm = generate_ccm_deposit();

			assert_ok!(Swapping::on_ccm_deposit(
				Asset::Usdc,
				100_000,
				Asset::Eth,
				eth_address,
				ccm,
				SwapOrigin::Vault { tx_hash: Default::default() },
			));

			// Change the swap rate so swap output will be 0
			SwapRate::set(0.0001f64);
			System::reset_events();
		})
		.then_process_blocks_until(|_| System::block_number() == 4)
		.then_execute_with(|_| {
			// Swap outputs are zero
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: 1,
					source_asset: Asset::Usdc,
					destination_asset: Asset::Eth,
					deposit_amount: 99_000,
					egress_amount: 9,
					swap_input: 99_000,
					swap_output: 9,
					intermediate_amount: None,
				}),
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: 2,
					source_asset: Asset::Usdc,
					deposit_amount: 1_000,
					destination_asset: Asset::Eth,
					egress_amount: 0,
					swap_input: 1_000,
					swap_output: 0,
					intermediate_amount: None,
				}),
			);

			// CCM are processed and egressed even if principal output is zero.
			assert_eq!(MockEgressHandler::<AnyChain>::get_scheduled_egresses().len(), 1);
			assert_swaps_queue_is_empty();
		});
}

#[test]
fn can_handle_swaps_with_zero_outputs() {
	new_test_ext()
		.then_execute_at_next_block(|_| {
			let eth_address = ForeignChainAddress::Eth(Default::default());

			Swapping::schedule_swap_from_channel(
				eth_address.clone(),
				Default::default(),
				Asset::Usdc,
				Asset::Eth,
				100,
				eth_address.clone(),
				Default::default(),
				0,
				0,
			);
			Swapping::schedule_swap_from_channel(
				eth_address.clone(),
				Default::default(),
				Asset::Usdc,
				Asset::Eth,
				1,
				eth_address,
				Default::default(),
				0,
				0,
			);

			// Change the swap rate so swap output will be 0
			SwapRate::set(0.01f64);
			System::reset_events();
		})
		.then_process_blocks_until(|_| System::block_number() == 4)
		.then_execute_with(|_| {
			// Swap outputs are zero
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: 1,
					destination_asset: Asset::Eth,
					swap_output: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressIgnored { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::<Test>::SwapExecuted {
					swap_id: 2,
					destination_asset: Asset::Eth,
					swap_output: 0,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressIgnored { swap_id: 2, .. }),
			);

			// Swaps are not egressed when output is 0.
			assert_swaps_queue_is_empty();
			assert!(
				MockEgressHandler::<AnyChain>::get_scheduled_egresses().is_empty(),
				"No egresses should be scheduled."
			);
		});
}

#[test]
fn can_set_maximum_swap_amount() {
	new_test_ext().execute_with(|| {
		let asset = Asset::Eth;
		let amount = Some(1_000u128);
		assert!(MaximumSwapAmount::<Test>::get(asset).is_none());

		// Set the new maximum swap_amount
		set_maximum_swap_amount(asset, amount);

		assert_eq!(MaximumSwapAmount::<Test>::get(asset), amount);
		assert_eq!(Swapping::maximum_swap_amount(asset), amount);

		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::MaximumSwapAmountSet {
			asset,
			amount,
		}));

		// Can remove maximum swap amount
		set_maximum_swap_amount(asset, None);
		assert!(MaximumSwapAmount::<Test>::get(asset).is_none());
		System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::MaximumSwapAmountSet {
			asset,
			amount: None,
		}));
	});
}

#[test]
fn swap_excess_are_confiscated_ccm_via_deposit() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 1_000;
		let max_swap = 100;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let request_ccm = generate_ccm_channel();
		let ccm = generate_ccm_deposit();

		set_maximum_swap_amount(from, Some(max_swap));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			EncodedAddress::Eth(Default::default()),
			0,
			Some(request_ccm),
			0,
		));

		assert_ok!(Swapping::on_ccm_deposit(
			from,
			gas_budget + principal_amount,
			to,
			ForeignChainAddress::Eth(Default::default()),
			ccm.clone(),
			SwapOrigin::Vault { tx_hash: Default::default() },
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_id: 1,
			source_asset: from,
			destination_asset: to,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_id: 2,
			source_asset: from,
			destination_asset: Asset::Eth,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap {
					swap_id: 1u64,
					from,
					to,
					amount: max_swap,
					swap_type: SwapType::CcmPrincipal(1),
					stable_amount: Some(max_swap),
					final_output: None,
					fee_taken: false,
				},
				Swap {
					swap_id: 2u64,
					from,
					to: Asset::Eth,
					amount: max_swap,
					swap_type: SwapType::CcmGas(1),
					stable_amount: Some(max_swap),
					final_output: None,
					fee_taken: false,
				}
			]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900 * 2);
	});
}

#[test]
fn swap_excess_are_confiscated_ccm_via_extrinsic() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 1_000;
		let max_swap = 100;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let ccm = generate_ccm_deposit();

		set_maximum_swap_amount(from, Some(max_swap));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			gas_budget + principal_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
			Default::default(),
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_id: 1,
			source_asset: from,
			destination_asset: to,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_id: 2,
			source_asset: from,
			destination_asset: Asset::Eth,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);
		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap {
					swap_id: 1u64,
					from,
					to,
					amount: max_swap,
					swap_type: SwapType::CcmPrincipal(1),
					stable_amount: Some(max_swap),
					final_output: None,
					fee_taken: false,
				},
				Swap {
					swap_id: 2u64,
					from,
					to: Asset::Eth,
					amount: max_swap,
					swap_type: SwapType::CcmGas(1),
					stable_amount: Some(max_swap),
					final_output: None,
					fee_taken: false,
				}
			]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900 * 2);
	});
}

#[test]
fn swap_excess_are_confiscated_for_swap_via_extrinsic() {
	new_test_ext().execute_with(|| {
		let max_swap = 100;
		let amount = 1_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		set_maximum_swap_amount(from, Some(max_swap));

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_id: 1,
			source_asset: from,
			destination_asset: to,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap {
				swap_id: 1u64,
				from,
				to,
				amount: max_swap,
				swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
				stable_amount: Some(max_swap),
				final_output: None,
				fee_taken: false,
			}]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900);
	});
}

#[test]
fn swap_excess_are_confiscated_for_swap_via_deposit() {
	new_test_ext().execute_with(|| {
		let max_swap = 100;
		let amount = 1_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		set_maximum_swap_amount(from, Some(max_swap));

		Swapping::schedule_swap_from_channel(
			ForeignChainAddress::Eth(Default::default()),
			1,
			from,
			to,
			amount,
			ForeignChainAddress::Eth(Default::default()),
			ALICE,
			0,
			0,
		);

		// Excess fee is confiscated
		System::assert_has_event(RuntimeEvent::Swapping(Event::<Test>::SwapAmountConfiscated {
			swap_id: 1,
			source_asset: from,
			destination_asset: to,
			total_amount: 1_000,
			confiscated_amount: 900,
		}));

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap {
				swap_id: 1u64,
				from,
				to,
				amount: max_swap,
				swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
				stable_amount: Some(max_swap),
				final_output: None,
				fee_taken: false,
			}]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900);
	});
}

#[test]
fn max_swap_amount_can_be_removed() {
	new_test_ext().execute_with(|| {
		let max_swap = 100;
		let amount = 1_000;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		// Initial max swap amount is set.
		set_maximum_swap_amount(from, Some(max_swap));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 900u128);

		// Reset event and confiscated funds.
		CollectedRejectedFunds::<Test>::set(from, 0u128);
		System::reset_events();

		// Max is removed.
		set_maximum_swap_amount(from, None);

		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap {
					swap_id: 1u64,
					from,
					to,
					amount: max_swap,
					swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
					stable_amount: Some(max_swap),
					final_output: None,
					fee_taken: false,
				},
				// New swap takes the full amount.
				Swap {
					swap_id: 2u64,
					from,
					to,
					amount,
					swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
					stable_amount: Some(amount),
					final_output: None,
					fee_taken: false,
				}
			]
		);
		// No no funds are confiscated.
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

#[test]
fn can_swap_below_max_amount() {
	new_test_ext().execute_with(|| {
		let max_swap = 1_001u128;
		let amount = 1_000u128;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;

		// Initial max swap amount is set.
		set_maximum_swap_amount(from, Some(max_swap));
		assert_ok!(Swapping::schedule_swap_from_contract(
			RuntimeOrigin::signed(ALICE),
			from,
			to,
			amount,
			EncodedAddress::Eth(Default::default()),
			Default::default(),
		));

		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0u128);

		assert_eq!(
			SwapQueue::<Test>::get(System::block_number() + u64::from(SWAP_DELAY_BLOCKS)),
			vec![Swap {
				swap_id: 1u64,
				from,
				to,
				amount,
				swap_type: SwapType::Swap(ForeignChainAddress::Eth(Default::default())),
				stable_amount: Some(amount),
				final_output: None,
				fee_taken: false,
			},]
		);
	});
}

#[test]
fn can_swap_ccm_below_max_amount() {
	new_test_ext().execute_with(|| {
		let gas_budget = GAS_BUDGET;
		let principal_amount = 999;
		let max_swap = 1_001;
		let from: Asset = Asset::Usdc;
		let to: Asset = Asset::Flip;
		let ccm = generate_ccm_deposit();

		set_maximum_swap_amount(from, Some(max_swap));

		// Register CCM via Swap deposit
		assert_ok!(Swapping::ccm_deposit(
			RuntimeOrigin::root(),
			from,
			gas_budget + principal_amount,
			to,
			EncodedAddress::Eth(Default::default()),
			ccm,
			Default::default(),
		));

		let execute_at = System::block_number() + u64::from(SWAP_DELAY_BLOCKS);

		assert_eq!(
			SwapQueue::<Test>::get(execute_at),
			vec![
				Swap {
					swap_id: 1u64,
					from,
					to,
					amount: principal_amount,
					swap_type: SwapType::CcmPrincipal(1),
					stable_amount: Some(principal_amount),
					final_output: None,
					fee_taken: false,
				},
				Swap {
					swap_id: 2u64,
					from,
					to: Asset::Eth,
					amount: gas_budget,
					swap_type: SwapType::CcmGas(1),
					stable_amount: Some(gas_budget),
					final_output: None,
					fee_taken: false,
				}
			]
		);
		assert_eq!(CollectedRejectedFunds::<Test>::get(from), 0);
	});
}

fn swap_with_custom_broker_fee(
	from: Asset,
	to: Asset,
	amount: AssetAmount,
	broker_fee: BasisPoints,
) {
	<Pallet<Test> as SwapDepositHandler>::schedule_swap_from_channel(
		ForeignChainAddress::Eth([2; 20].into()),
		Default::default(),
		from,
		to,
		amount,
		ForeignChainAddress::Eth([2; 20].into()),
		ALICE,
		broker_fee,
		1,
	);
}

#[test]
fn swap_broker_fee_calculated_correctly() {
	new_test_ext().execute_with(|| {
		let fees: [BasisPoints; 12] =
			[1, 5, 10, 100, 200, 500, 1000, 1500, 2000, 5000, 7500, 10000];
		const AMOUNT: AssetAmount = 100000;

		// calculate broker fees for each asset available
		Asset::all().for_each(|asset| {
			let total_fees: u128 =
				fees.iter().fold(0, |total_fees: u128, fee_bps: &BasisPoints| {
					swap_with_custom_broker_fee(asset, Asset::Usdc, AMOUNT, *fee_bps);
					total_fees +
						Permill::from_parts(*fee_bps as u32 * BASIS_POINTS_PER_MILLION) * AMOUNT
				});
			assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, asset), total_fees);
		});
	});
}

#[test]
fn swap_broker_fee_cannot_exceed_amount() {
	new_test_ext().execute_with(|| {
		swap_with_custom_broker_fee(Asset::Usdc, Asset::Flip, 100, 15000);
		assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, cf_primitives::Asset::Usdc), 100);
	});
}

fn assert_swap_scheduled_event_emitted(
	swap_id: u64,
	source_asset: Asset,
	deposit_amount: AssetAmount,
	destination_asset: Asset,
	broker_commission: AssetAmount,
	execute_at: u64,
) {
	System::assert_last_event(RuntimeEvent::Swapping(Event::<Test>::SwapScheduled {
		swap_id,
		source_asset,
		deposit_amount,
		destination_asset,
		destination_address: EncodedAddress::Eth([2; 20]),
		origin: SwapOrigin::DepositChannel {
			deposit_address: EncodedAddress::Eth([2; 20]),
			channel_id: 1,
			deposit_block_height: Default::default(),
		},
		swap_type: SwapType::Swap(ForeignChainAddress::Eth([2; 20].into())),
		broker_commission: Some(broker_commission),
		execute_at,
	}));
}
#[test]
fn swap_broker_fee_subtracted_from_swap_amount() {
	new_test_ext().execute_with(|| {
		let amounts: [AssetAmount; 6] = [50, 100, 200, 500, 1000, 10000];
		let fees: [BasisPoints; 4] = [100, 1000, 5000, 10000];

		let combinations = amounts.iter().cartesian_product(fees);

		let execute_at = System::block_number() + SWAP_DELAY_BLOCKS as u64;

		let mut swap_id = 1;
		Asset::all().for_each(|asset| {
			let mut total_fees = 0;
			combinations.clone().for_each(|(amount, broker_fee)| {
				swap_with_custom_broker_fee(asset, Asset::Flip, *amount, broker_fee);
				let broker_commission =
					Permill::from_parts(broker_fee as u32 * BASIS_POINTS_PER_MILLION) * *amount;
				total_fees += broker_commission;
				assert_eq!(EarnedBrokerFees::<Test>::get(ALICE, asset), total_fees);
				assert_swap_scheduled_event_emitted(
					swap_id,
					asset,
					*amount,
					Asset::Flip,
					broker_commission,
					execute_at,
				);
				swap_id += 1;
			})
		});
	});
}

#[test]
fn broker_bps_is_limited() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Swapping::request_swap_deposit_address(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				Asset::Usdc,
				EncodedAddress::Eth(Default::default()),
				1001,
				None,
				0,
			),
			Error::<Test>::BrokerCommissionBpsTooHigh
		);
	});
}

#[test]
fn swaps_are_executed_according_to_execute_at_field() {
	let mut swaps = generate_test_swaps();
	let later_swaps = swaps.split_off(2);

	new_test_ext()
		.execute_with(|| {
			// Block 1, swaps should be scheduled at block 3
			assert_eq!(System::block_number(), 1);
			assert_eq!(FirstUnprocessedBlock::<Test>::get(), 0);
			insert_swaps(&swaps);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 1, execute_at: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 2, execute_at: 3, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// Block 2, swaps should be scheduled at block 4
			assert_eq!(System::block_number(), 2);
			insert_swaps(&later_swaps);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 3, execute_at: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 4, execute_at: 4, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// First group of swaps will be processed at the end of this block
			assert_eq!(FirstUnprocessedBlock::<Test>::get(), 3);
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 3);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 2, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// Second group of swaps will be processed at the end of this block
			assert_eq!(FirstUnprocessedBlock::<Test>::get(), 4);
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 4);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 4, .. }),
			);
		});
}

#[test]
fn swaps_get_retried_on_next_block_after_failure() {
	let mut swaps = generate_test_swaps();
	let later_swaps = swaps.split_off(2);

	new_test_ext()
		.execute_with(|| {
			// Block 1, swaps should be scheduled at block 3
			assert_eq!(System::block_number(), 1);
			assert_eq!(FirstUnprocessedBlock::<Test>::get(), 0);
			insert_swaps(&swaps);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 1, execute_at: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 2, execute_at: 3, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// Block 2, swaps should be scheduled at block 4
			assert_eq!(System::block_number(), 2);
			insert_swaps(&later_swaps);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 3, execute_at: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapScheduled { swap_id: 4, execute_at: 4, .. }),
			);
		})
		.then_execute_at_next_block(|_| {
			// First group of swaps will be processed at the end of this block,
			// but we force them to fail:
			MockSwappingApi::set_swaps_should_fail(true);
			assert_eq!(FirstUnprocessedBlock::<Test>::get(), 3);
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 3);
			assert_event_sequence!(Test, RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),);

			// The storage state has been rolled back:
			assert_eq!(SwapQueue::<Test>::get(3).len(), 2);
		})
		.then_execute_at_next_block(|_| {
			// All swaps be processed at the end of this block, including the swaps
			// from previous block. This time we allow swaps to succeed:
			MockSwappingApi::set_swaps_should_fail(false);
			assert_eq!(FirstUnprocessedBlock::<Test>::get(), 3);
		})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), 4);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 4, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 4, .. }),
			);
		});
}

#[test]
fn deposit_address_ready_event_contain_correct_boost_fee_value() {
	new_test_ext().execute_with(|| {
		const BOOST_FEE: u16 = 100;
		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			Asset::Usdc,
			EncodedAddress::Eth(Default::default()),
			0,
			None,
			BOOST_FEE
		));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapDepositAddressReady { boost_fee: BOOST_FEE, .. })
		);
	});
}

#[test]
fn can_update_multiple_items_at_once() {
	new_test_ext().execute_with(|| {
		assert!(MaximumSwapAmount::<Test>::get(Asset::Btc).is_none());
		assert!(MaximumSwapAmount::<Test>::get(Asset::Dot).is_none());
		assert_ok!(Swapping::update_pallet_config(
			OriginTrait::root(),
			vec![
				PalletConfigUpdate::MaximumSwapAmount { asset: Asset::Btc, amount: Some(100) },
				PalletConfigUpdate::MaximumSwapAmount { asset: Asset::Dot, amount: Some(200) },
			]
			.try_into()
			.unwrap()
		));
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Btc), Some(100));
		assert_eq!(MaximumSwapAmount::<Test>::get(Asset::Dot), Some(200));
	});
}
