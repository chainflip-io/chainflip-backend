const GAS_ASSET: Asset = Asset::Eth;

use super::*;
use sp_core::H256;

#[track_caller]
fn init_ccm_swap_request(input_asset: Asset, output_asset: Asset, input_amount: AssetAmount) {
	let ccm_deposit_metadata = generate_ccm_deposit();
	let output_address = (*EVM_OUTPUT_ADDRESS).clone();
	let encoded_output_address = MockAddressConverter::to_encoded_address(output_address.clone());
	let origin = SwapOrigin::Vault { tx_id: TransactionInIdForAnyChain::ByteHash(H256::default()) };

	let ccm_swap_metadata = ccm_deposit_metadata
		.into_swap_metadata(input_amount, input_asset, output_asset)
		.unwrap();

	Swapping::init_swap_request(
		input_asset,
		input_amount,
		output_asset,
		SwapRequestType::Ccm { ccm_swap_metadata: ccm_swap_metadata.clone(), output_address },
		Default::default(),
		None,
		None,
		origin.clone(),
	);

	System::assert_has_event(RuntimeEvent::Swapping(Event::SwapRequested {
		swap_request_id: SWAP_REQUEST_ID,
		input_asset,
		output_asset,
		input_amount,
		request_type: SwapRequestTypeEncoded::Ccm {
			ccm_swap_metadata: ccm_swap_metadata
				.to_encoded::<<Test as pallet::Config>::AddressConverter>(),
			output_address: encoded_output_address,
		},
		dca_parameters: None,
		refund_parameters: None,
		origin,
	}));
}

#[track_caller]
pub(super) fn assert_ccm_egressed(
	asset: Asset,
	principal_amount: AssetAmount,
	gas_budget: AssetAmount,
) {
	assert_has_matching_event!(
		Test,
		RuntimeEvent::Swapping(Event::<Test>::SwapEgressScheduled {
			swap_request_id: SWAP_REQUEST_ID,
			..
		})
	);

	let ccm_egress = MockEgressHandler::<AnyChain>::get_scheduled_egresses()
		.into_iter()
		.find(|egress| matches!(egress, MockEgressParameter::Ccm { .. }))
		.expect("no ccm egress");

	assert_eq!(
		ccm_egress,
		MockEgressParameter::Ccm {
			asset,
			amount: principal_amount,
			destination_address: (*EVM_OUTPUT_ADDRESS).clone(),
			message: vec![0x01].try_into().unwrap(),
			ccm_additional_data: vec![].try_into().unwrap(),
			gas_budget,
		},
	);
}

#[test]
fn can_process_ccms_via_swap_deposit_address() {
	const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const GAS_SWAP_BLOCK: u64 = PRINCIPAL_SWAP_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const DEPOSIT_AMOUNT: AssetAmount = 10_000;

	new_test_ext()
		.execute_with(|| {
			let request_ccm = generate_ccm_channel();

			let ccm_swap_metadata = generate_ccm_deposit()
				.into_swap_metadata(DEPOSIT_AMOUNT, Asset::Dot, Asset::Eth)
				.unwrap();

			// Can process CCM via Swap deposit
			assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(BROKER),
				Asset::Dot,
				Asset::Eth,
				MockAddressConverter::to_encoded_address((*EVM_OUTPUT_ADDRESS).clone()),
				0,
				Some(request_ccm),
				0,
				Default::default(),
				None,
				None,
			));

			Swapping::init_swap_request(
				Asset::Dot,
				DEPOSIT_AMOUNT,
				Asset::Eth,
				SwapRequestType::Ccm {
					ccm_swap_metadata: ccm_swap_metadata.clone(),
					output_address: (*EVM_OUTPUT_ADDRESS).clone(),
				},
				Default::default(),
				None,
				None,
				SwapOrigin::Vault { tx_id: TransactionInIdForAnyChain::ByteHash(H256::default()) },
			);

			// Principal swap is scheduled first
			assert_eq!(
				SwapQueue::<Test>::get(PRINCIPAL_SWAP_BLOCK),
				vec![Swap::new(
					1.into(),
					1.into(),
					Asset::Dot,
					Asset::Eth,
					DEPOSIT_AMOUNT - GAS_BUDGET,
					None,
					[FeeType::NetworkFee],
				),]
			);
		})
		.then_process_blocks_until_block(PRINCIPAL_SWAP_BLOCK)
		.then_execute_with(|_| {
			// Gas swap should only be scheduled after principal is executed
			assert_eq!(
				SwapQueue::<Test>::get(GAS_SWAP_BLOCK),
				vec![Swap::new(
					2.into(),
					1.into(),
					Asset::Dot,
					Asset::Eth,
					GAS_BUDGET,
					None,
					[FeeType::NetworkFee],
				),]
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
			);
		})
		.then_process_blocks_until_block(GAS_SWAP_BLOCK)
		.then_execute_with(|_| {
			// CCM is scheduled for egress
			assert_ccm_egressed(
				Asset::Eth,
				(DEPOSIT_AMOUNT - GAS_BUDGET) * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
				GAS_BUDGET * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(2), .. }),
			);
		});
}

#[test]
fn ccm_no_swap() {
	const PRINCIPAL_AMOUNT: AssetAmount = 10_000;
	const SWAP_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT + GAS_BUDGET;

	// Both input and output assets are Eth, so no swap is needed:
	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Eth;
	new_test_ext().execute_with(|| {
		init_ccm_swap_request(INPUT_ASSET, OUTPUT_ASSET, SWAP_AMOUNT);

		// No need to store the request in this case:
		assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

		// CCM should be immediately egressed:
		assert_ccm_egressed(OUTPUT_ASSET, PRINCIPAL_AMOUNT, GAS_BUDGET);

		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapRequestCompleted {
				swap_request_id: SWAP_REQUEST_ID,
				..
			}),
		);

		assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
		assert_eq!(CollectedRejectedFunds::<Test>::get(OUTPUT_ASSET), 0);
	});
}

#[test]
fn ccm_principal_swap_only() {
	const PRINCIPAL_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const PRINCIPAL_AMOUNT: AssetAmount = 10_000;
	const SWAP_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT + GAS_BUDGET;

	// Gas asset is Eth, so no gas swap is necessary
	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Flip;
	new_test_ext()
		.execute_with(|| {
			init_ccm_swap_request(INPUT_ASSET, OUTPUT_ASSET, SWAP_AMOUNT);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			// Principal swap should be immediately scheduled
			assert_eq!(
				SwapQueue::<Test>::get(PRINCIPAL_SWAP_BLOCK),
				vec![Swap::new(
					1.into(),
					SWAP_REQUEST_ID,
					INPUT_ASSET,
					OUTPUT_ASSET,
					PRINCIPAL_AMOUNT,
					None,
					[FeeType::NetworkFee],
				),]
			);
		})
		.then_process_blocks_until_block(PRINCIPAL_SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID,
					..
				}),
			);

			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_ccm_egressed(
				OUTPUT_ASSET,
				PRINCIPAL_AMOUNT * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
				GAS_BUDGET,
			);

			assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
			assert_eq!(CollectedRejectedFunds::<Test>::get(OUTPUT_ASSET), 0);
		});
}

#[test]
fn ccm_gas_swap_only() {
	const GAS_SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const INPUT_ASSET: Asset = Asset::Flip;
	const OUTPUT_ASSET: Asset = Asset::Usdc;
	new_test_ext()
		.execute_with(|| {
			// Ccm with principal asset = 0
			init_ccm_swap_request(INPUT_ASSET, OUTPUT_ASSET, GAS_BUDGET);

			assert!(SwapRequests::<Test>::get(SWAP_REQUEST_ID).is_some());

			// Gas swap should be immediately scheduled
			assert_eq!(
				SwapQueue::<Test>::get(GAS_SWAP_BLOCK),
				vec![Swap::new(
					1.into(),
					SWAP_REQUEST_ID,
					INPUT_ASSET,
					GAS_ASSET,
					GAS_BUDGET,
					None,
					[FeeType::NetworkFee]
				),]
			);
		})
		.then_process_blocks_until_block(GAS_SWAP_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: SwapId(1), .. }),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID,
					..
				}),
			);

			assert_ccm_egressed(
				OUTPUT_ASSET,
				0,
				GAS_BUDGET * DEFAULT_SWAP_RATE * DEFAULT_SWAP_RATE,
			);

			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_eq!(CollectedRejectedFunds::<Test>::get(INPUT_ASSET), 0);
			assert_eq!(CollectedRejectedFunds::<Test>::get(OUTPUT_ASSET), 0);
		});
}
