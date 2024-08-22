use core::u128;

use super::*;

const INPUT_AMOUNT: AssetAmount = 40_000;
const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
const NET_AMOUNT: AssetAmount = INPUT_AMOUNT - BROKER_FEE;

#[derive(Debug, Clone)]
struct DcaTestParams {
	input_amount: AssetAmount,
	dca_params: DcaParameters,
	refund_params: Option<TestRefundParams>,
	is_ccm: bool,
}

impl DcaTestParams {
	fn new(number_of_chunks: u32) -> Self {
		Self {
			input_amount: INPUT_AMOUNT,
			dca_params: DcaParameters { number_of_chunks, chunk_interval: 10 },
			refund_params: None,
			is_ccm: false,
		}
	}

	fn with_refund_params(mut self, params: SwapRefundParameters) -> Self {
		self.refund_params.insert(params);
		self
	}

	fn num_chunks(&self) -> u32 {
		self.dca_params.number_of_chunks
	}

	fn chunk_interval(&self) -> u32 {
		self.dca_params.chunk_interval
	}

	fn chunk_size(&self) -> AssetAmount {
		NET_AMOUNT / self.num_chunks() as u128
	}

	fn expected_execution_block(&self, chunk: u32) -> u64 {
		INIT_BLOCK + (SWAP_DELAY_BLOCKS + (chunk - 1) * self.chunk_interval()) as u64
	}
}

impl From<DcaTestParams> for TestSwapParams {
	fn from(params: DcaTestParams) -> Self {
		const INPUT_ASSET: Asset = Asset::Usdc;
		const OUTPUT_ASSET: Asset = Asset::Eth;

		TestSwapParams {
			input_asset: INPUT_ASSET,
			output_asset: OUTPUT_ASSET,
			input_amount: params.input_amount,
			refund_params: params.refund_params: refund_params.map(|params| params.into_channel_params(INPUT_AMOUNT)),
			dca_params: Some(params.dca_params),
			output_address: (*EVM_OUTPUT_ADDRESS).clone(),
			is_ccm: params.is_ccm,
		}
	}
}

trait DcaTestsSetup {
	fn setup(self, swaps: DcaTestParams) -> TestRunner<DcaTestParams>;
}
trait DcaTestsExt {
	fn expect_chunk_execution(self, chunk: u32) -> TestRunner<DcaTestParams>;
}

impl DcaTestsSetup for TestRunner<()> {
	fn setup(self, params: DcaTestParams) -> TestRunner<DcaTestParams> {
		self.execute_with(move || {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params.into()]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested { input_amount, .. }) if *input_amount == params.input_amount
			);

			let swap_request_id = assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id,
					swap_id: 1,
					input_amount,
					execute_at,
					..
				}) if *input_amount == params.chunk_size() && *execute_at == params.chunk_interval() as u64
				=> swap_request_id
			);

			assert_eq!(
				get_dca_state(swap_request_id),
				DcaState {
					scheduled_chunk_swap_id: Some(1),
					remaining_input_amount: params.chunk_size(),
					remaining_chunks: 1,
					chunk_interval: params.chunk_interval(),
					accumulated_output_amount: 0
				}
			);
			params
		})
	}
}

impl DcaTestsExt for TestRunner<DcaTestParams> {
	fn expect_chunk_execution(self, chunk: u32) -> Self {
		self.then_process_blocks_until(|test_params| {
			System::block_number() >= test_params.expected_execution_block(chunk) ||
				// TODO: a more robust limit
				System::block_number() > 1_000
		})
		.then_process_events(|test_params, event| {
			match event {
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id,
					swap_id,
					input_amount,
					..
				}) => {
					assert_eq!(input_amount, test_params.chunk_size());
					assert_eq!(swap_request_id, SWAP_REQUEST_ID);
					assert_eq!(swap_id, chunk as u64);
				},
				_ => {},
			}
			None::<()>
		})
		.map_context(|(test_params, _)| test_params)
	}
}

fn get_dca_state(request_id: SwapRequestId) -> DcaState {
	match SwapRequests::<Test>::get(request_id)
		.expect("request state does not exist")
		.state
	{
		SwapRequestState::Ccm { dca_state, .. } => dca_state,
		SwapRequestState::Regular { dca_state, .. } => dca_state,
		other => {
			panic!("DCA not supported for {other:?}")
		},
	}
}

fn get_ccm_gas_state(request_id: SwapRequestId) -> GasSwapState {
	if let SwapRequestState::Ccm { gas_swap_state, .. } = SwapRequests::<Test>::get(request_id)
		.expect("request state does not exist")
		.state
	{
		gas_swap_state
	} else {
		panic!("Not a CCM swap");
	}
}

#[test]
fn dca_happy_path() {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / 2;
	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT * DEFAULT_SWAP_RATE;

	// 1:1 swap ratio
	const TOTAL_OUTPUT_AMOUNT: AssetAmount = NET_AMOUNT * DEFAULT_SWAP_RATE;

	new_test_ext()
		.setup(DcaTestParams::new(2))
		.expect_chunk_execution(1)
		.then_process_events(|test_params, event| {
			match event {
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id,
					swap_id,
					input_amount,
					execute_at,
					..
				}) => {
					assert_eq!(input_amount, test_params.chunk_size());
					assert_eq!(swap_request_id, SWAP_REQUEST_ID);
					assert_eq!(swap_id, 2);
				},
				_ => {},
			};
			None::<()>
		})
		.map_context(|(test_params, _)| test_params)
		.then_process_blocks_until(|test_params| {
			System::block_number() >= test_params.expected_execution_block(2)
		})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 2,
					input_amount: CHUNK_AMOUNT,
					output_amount: CHUNK_OUTPUT,
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

	const EGRESS_AMOUNT: AssetAmount = NET_AMOUNT * DEFAULT_SWAP_RATE;

	new_test_ext()
		.setup(DcaTestParams::new(1))
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: NET_AMOUNT,
					output_amount: EGRESS_AMOUNT,
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
	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / NUMBER_OF_CHUNKS as u128;

	// Allow for one retry for good measure:
	const REFUND_BLOCK: u64 = CHUNK_1_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	new_test_ext()
		.setup(DcaTestParams::new(NUMBER_OF_CHUNKS).with_refund_params(TestRefundParams {
					// Allow for exactly 1 retry
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					// This ensures the swap is refunded:
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
				}))
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
					amount: NET_AMOUNT,
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
	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / NUMBER_OF_CHUNKS as u128;
	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT * DEFAULT_SWAP_RATE;

	// The test will be set up as to execute one chunk only and refund the rest
	const REFUNDED_AMOUNT: AssetAmount = NET_AMOUNT - CHUNK_AMOUNT;

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
					input_amount: CHUNK_AMOUNT,
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
	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / NUMBER_OF_CHUNKS as u128;

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
			SwapRate::set(1.0);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					output_amount: CHUNK_AMOUNT,
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
					accumulated_output_amount: CHUNK_AMOUNT
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
					input_amount: CHUNK_AMOUNT,
					output_amount: CHUNK_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					// full amount should be egressed:
					amount: NET_AMOUNT,
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

	const PRINCIPAL_AMOUNT: AssetAmount = NET_AMOUNT - GAS_BUDGET;

	const CHUNK_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT / 2;

	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT * DEFAULT_SWAP_RATE;

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
					input_amount: CHUNK_AMOUNT,
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
					accumulated_output_amount: CHUNK_AMOUNT * DEFAULT_SWAP_RATE
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
					input_amount: CHUNK_AMOUNT,
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

			ccm::assert_ccm_egressed(
				OUTPUT_ASSET,
				PRINCIPAL_AMOUNT * DEFAULT_SWAP_RATE,
				GAS_BUDGET * DEFAULT_SWAP_RATE,
			)
		});
}

// TODO: once FoK is implemented for CCM, test full and partial refunds in CCM with DCA
