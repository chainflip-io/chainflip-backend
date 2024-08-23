use super::*;

const INPUT_AMOUNT: AssetAmount = 40_000;
const CHUNK_INTERVAL: u32 = 3;

const INPUT_ASSET: Asset = Asset::Usdc;
const OUTPUT_ASSET: Asset = Asset::Eth;

fn params(
	dca_params: Option<DcaParameters>,
	refund_params: Option<TestRefundParams>,
	is_ccm: bool,
) -> TestSwapParams {
	TestSwapParams {
		input_asset: INPUT_ASSET,
		output_asset: OUTPUT_ASSET,
		input_amount: INPUT_AMOUNT,
		refund_params: refund_params.map(|params| params.into_channel_params(INPUT_AMOUNT)),
		dca_params,
		output_address: (*EVM_OUTPUT_ADDRESS).clone(),
		is_ccm,
	}
}

fn get_dca_state(request_id: SwapRequestId) -> DcaState {
	match SwapRequests::<Test>::get(request_id)
		.expect("request state does not exist")
		.state
	{
		SwapRequestState::UserSwap { dca_state, .. } => dca_state,
		other => {
			panic!("DCA not supported for {other:?}")
		},
	}
}

fn get_ccm_gas_state(request_id: SwapRequestId) -> GasSwapState {
	if let SwapRequestState::UserSwap { ccm: Some(ccm), .. } = SwapRequests::<Test>::get(request_id)
		.expect("request state does not exist")
		.state
	{
		ccm.gas_swap_state
	} else {
		panic!("Not a CCM swap");
	}
}

#[test]
fn dca_happy_path() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;

	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	const TOTAL_OUTPUT_AMOUNT: AssetAmount = CHUNK_OUTPUT * 2;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			const DCA_PARAMS: DcaParameters =
				DcaParameters { number_of_chunks: 2, chunk_interval: CHUNK_INTERVAL };

			insert_swaps(&[params(Some(DCA_PARAMS), None, false)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					dca_parameters: Some(DCA_PARAMS),
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					..
				})
			);

			// Second chunk should be scheduled 2 blocks after the first is executed:
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(2),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);
		})
		.then_execute_at_block(CHUNK_2_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					broker_fee: CHUNK_BROKER_FEE,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					amount: TOTAL_OUTPUT_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);
		});
}

/// Test that DCA with 1 chunk behaves like a regular swap
#[test]
fn dca_single_chunk() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const INPUT_AMOUNT_AFTER_FEE: AssetAmount = INPUT_AMOUNT - BROKER_FEE;
	const EGRESS_AMOUNT: AssetAmount = INPUT_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters { number_of_chunks: 1, chunk_interval: CHUNK_INTERVAL }),
				None,
				false,
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: INPUT_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: INPUT_AMOUNT_AFTER_FEE,
					output_amount: EGRESS_AMOUNT,
					broker_fee: BROKER_FEE,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					amount: EGRESS_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				})
			);
		});
}

#[test]
fn dca_with_fok_full_refund() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const NUMBER_OF_CHUNKS: u32 = 2;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;

	// Allow for one retry for good measure:
	const REFUND_BLOCK: u64 = CHUNK_1_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: CHUNK_INTERVAL,
				}),
				Some(TestRefundParams {
					// Allow for exactly 1 retry
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					// This ensures the swap is refunded:
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
				}),
				false,
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 1,
					execute_at: REFUND_BLOCK
				})
			);

			// Note that there is no change to the DCA state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(REFUND_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap should fail after the first retry and result in a
			// refund of the full input amount (rather than that of a chunk)
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: INPUT_ASSET,
					amount: INPUT_AMOUNT,
					..
				})
			);
		});
}

#[test]
fn dca_with_fok_partial_refund() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;
	const CHUNK_2_RESCHEDULED_AT_BLOCK: u64 =
		CHUNK_2_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	const NUMBER_OF_CHUNKS: u32 = 4;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;
	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	// The test will be set up as to execute one chunk only and refund the rest
	const REFUNDED_AMOUNT: AssetAmount = INPUT_AMOUNT - CHUNK_AMOUNT;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: CHUNK_INTERVAL,
				}),
				Some(TestRefundParams {
					// Allow for one retry for good measure:
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: INPUT_AMOUNT,
				}),
				false,
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: REFUNDED_AMOUNT,
					remaining_chunks: 3,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					..
				})
			);

			// Second chunk should be scheduled 2 blocks after the first is executed:
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(2),
					remaining_input_amount: REFUNDED_AMOUNT - CHUNK_AMOUNT,
					remaining_chunks: 2,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);
		})
		.then_execute_at_block(CHUNK_2_BLOCK, |_| {
			// Adjusting the swap rate, so that the second chunk fails due to FoK:
			SwapRate::set(0.5);
		})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 2,
					execute_at: CHUNK_2_RESCHEDULED_AT_BLOCK
				})
			);

			// Note that there is no change to the DCA state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(2),
					remaining_input_amount: REFUNDED_AMOUNT - CHUNK_AMOUNT,
					remaining_chunks: 2,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);
		})
		.then_execute_at_block(CHUNK_2_RESCHEDULED_AT_BLOCK, |_| {})
		.then_execute_with(|_| {
			// The swap will fail again, but this time it will reach expiry,
			// resulting in a refund of the remaining amount and egress of the
			// already executed amount.
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: INPUT_ASSET,
					amount: REFUNDED_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					amount: CHUNK_OUTPUT,
					..
				}),
			);
		});
}

#[test]
fn dca_with_fok_fully_executed() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_1_RETRY_BLOCK: u64 = CHUNK_1_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);
	const CHUNK_2_BLOCK: u64 = CHUNK_1_RETRY_BLOCK + CHUNK_INTERVAL as u64;
	const NUMBER_OF_CHUNKS: u32 = 2;

	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;
	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	const TOTAL_OUTPUT: AssetAmount = CHUNK_OUTPUT * 2;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: CHUNK_INTERVAL,
				}),
				Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: CHUNK_OUTPUT,
				}),
				false,
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {
			// Adjusting the swap rate, so that the first chunk fails at first
			SwapRate::set(0.5);
		})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 1,
					execute_at: CHUNK_1_RETRY_BLOCK,
					..
				})
			);

			// No change:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_execute_at_block(CHUNK_1_RETRY_BLOCK, |_| {
			// Set the price back to normal, so that the fist chunk is successful
			SwapRate::set(DEFAULT_SWAP_RATE as f64);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					broker_fee: CHUNK_BROKER_FEE,
					..
				}),
				// Second chunk should be scheduled 2 blocks after the first is executed:
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(2),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);
		})
		.then_execute_at_block(CHUNK_2_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					// Total amount from all chunks should be egressed:
					amount: TOTAL_OUTPUT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);
		});
}

#[test]
fn dca_with_ccm_happy_path() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

	// NOTE: gas swap is scheduled immediately after the first chunk,
	// and we apply the default swap delay to them (rather than chunk interval)
	const GAS_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const PRINCIPAL_AMOUNT: AssetAmount = INPUT_AMOUNT - GAS_BUDGET;

	const CHUNK_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT / 2;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;

	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters { number_of_chunks: 2, chunk_interval: CHUNK_INTERVAL }),
				None,
				true,
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);

			assert_eq!(
				get_ccm_gas_state(SWAP_REQUEST_ID),
				GasSwapState::ToBeScheduled {
					gas_budget: GAS_BUDGET,
					other_gas_asset: OUTPUT_ASSET
				}
			);
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Once the first chunk is successfully executed, gas swap should be
			// scheduled together with the second chunk:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					swap_type: SwapType::CcmPrincipal,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 3,
					input_amount: GAS_BUDGET,
					execute_at: GAS_BLOCK,
					swap_type: SwapType::CcmGas,
					..
				}),
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(2),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);

			assert_eq!(
				get_ccm_gas_state(SWAP_REQUEST_ID),
				GasSwapState::Scheduled { gas_swap_id: 3 }
			);
		})
		.then_execute_at_block(GAS_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 3,
					input_amount: GAS_BUDGET,
					output_amount,
					..
				}) if *output_amount == GAS_BUDGET * DEFAULT_SWAP_RATE,
			);

			// Gas swap has no effect on the DCA principal state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunk_swap_id: Some(2),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);

			assert_eq!(
				get_ccm_gas_state(SWAP_REQUEST_ID),
				GasSwapState::OutputReady { gas_budget: GAS_BUDGET * DEFAULT_SWAP_RATE }
			);
		})
		.then_execute_at_block(CHUNK_2_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					..
				}),
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);

			ccm::assert_ccm_egressed(OUTPUT_ASSET, CHUNK_OUTPUT * 2, GAS_BUDGET * DEFAULT_SWAP_RATE)
		});
}

// TODO: once FoK is implemented for CCM, test full and partial refunds in CCM with DCA
