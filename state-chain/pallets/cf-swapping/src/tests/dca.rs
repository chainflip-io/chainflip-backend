use core::u128;

use super::*;

const CHUNK_INTERVAL: u32 = 3;

#[derive(Debug, Clone)]
struct DcaTestParams {
	input_amount: AssetAmount,
	dca_params: DcaParameters,
	refund_params: Option<TestRefundParams>,
	is_ccm: bool,
}

impl DcaTestParams {
	fn new(number_of_chunks: u32, chunk_interval: u32) -> Self {
		Self {
			input_amount: INPUT_AMOUNT,
			dca_params: DcaParameters { number_of_chunks, chunk_interval },
			refund_params: None,
			is_ccm: false,
		}
	}

	fn is_ccm(mut self) -> Self {
		self.is_ccm = true;
		self
	}

	fn with_refund_params(mut self, params: TestRefundParams) -> Self {
		let _ = self.refund_params.insert(params);
		self
	}

	fn num_chunks(&self) -> u32 {
		if self.dca_params.number_of_chunks == 0 {
			1
		} else {
			self.dca_params.number_of_chunks
		}
	}

	fn chunk_interval(&self) -> u32 {
		self.dca_params.chunk_interval
	}

	fn chunk_size(&self) -> AssetAmount {
		if self.is_ccm {
			(self.input_amount - GAS_BUDGET) / self.num_chunks() as u128
		} else {
			self.input_amount / self.num_chunks() as u128
		}
	}

	fn chunk_input_amount(&self) -> AssetAmount {
		let fee = self.chunk_size() * BROKER_FEE_BPS as u128 / 10_000;
		self.chunk_size() - fee
	}

	fn expected_execution_block(&self, chunk: u32) -> u64 {
		INIT_BLOCK + (SWAP_DELAY_BLOCKS + (chunk - 1) * self.chunk_interval()) as u64
	}

	fn expected_remaining_amount(&self, chunk: u32) -> AssetAmount {
		if self.is_ccm {
			self.input_amount - GAS_BUDGET - self.chunk_size() * chunk as u128
		} else {
			self.input_amount - self.chunk_size() * chunk as u128
		}
	}

	fn expected_accumulated_amount(&self, chunk: u32) -> AssetAmount {
		// TODO JAMIE: why no swap rate?
		self.chunk_input_amount() * chunk as u128
	}

	fn expected_swap_id(&self, chunk: u32) -> SwapId {
		if !self.is_ccm || chunk <= 2 {
			chunk as u64
		} else {
			chunk as u64 + 1
		}
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
			refund_params: params
				.refund_params
				.map(|params| params.into_channel_params(INPUT_AMOUNT)),
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
	#[track_caller]
	fn setup(self, test_params: DcaTestParams) -> TestRunner<DcaTestParams> {
		self.execute_with(move || {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[test_params.clone().into()]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested { input_amount, .. }) if *input_amount == test_params.input_amount
			);

			let expected_swap_type =
					if test_params.is_ccm { SwapType::CcmPrincipal } else { SwapType::Swap };
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id,
					swap_id: 1,
					input_amount,
					execute_at,
					swap_type,
					..
				}) if *input_amount == test_params.chunk_size() &&
					*execute_at == INIT_BLOCK + SWAP_DELAY_BLOCKS as u64 &&
					*swap_request_id == SWAP_REQUEST_ID &&
					*swap_type == expected_swap_type
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(1),
					remaining_input_amount: test_params.expected_remaining_amount(test_params.dca_params.number_of_chunks-1),
					remaining_chunks: test_params.dca_params.number_of_chunks-1,
					chunk_interval: test_params.chunk_interval(),
					accumulated_output_amount: 0
				}
			);

			if test_params.is_ccm {
				assert_eq!(
					get_ccm_gas_state(SWAP_REQUEST_ID),
					GasSwapState::ToBeScheduled {
						gas_budget: GAS_BUDGET,
						other_gas_asset: OUTPUT_ASSET
					}
				);
			}
			test_params
		})
	}
}

impl DcaTestsExt for TestRunner<DcaTestParams> {
	#[track_caller]
	fn expect_chunk_execution(self, chunk: u32) -> Self {
		self.then_process_blocks_until(|test_params| {
			System::block_number() >= test_params.expected_execution_block(chunk)
		})
		.then_execute_with(|test_params| {
			// Check that the chunk was executed
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id,
					input_amount,
					..
				}) if *swap_id == test_params.expected_swap_id(chunk) && *input_amount == test_params.chunk_input_amount()
			);

			if chunk < test_params.dca_params.number_of_chunks {
				// Check that the next chunk was scheduled
				let next_chunk = chunk + 1;
				let expected_swap_type =
					if test_params.is_ccm { SwapType::CcmPrincipal } else { SwapType::Swap };
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id,
						input_amount,
						execute_at,
						swap_type,
						..
					}) if *swap_id == test_params.expected_swap_id(next_chunk) &&
						*input_amount == test_params.chunk_size() &&
						*execute_at == test_params.expected_execution_block(next_chunk) &&
						*swap_type == expected_swap_type
				);

				assert_eq!(
					get_dca_state(SWAP_REQUEST_ID),
					DcaState {
						status: DcaStatus::ChunkScheduled(next_chunk as u64),
						remaining_input_amount: test_params.expected_remaining_amount(next_chunk),
						remaining_chunks: test_params.dca_params.number_of_chunks - next_chunk,
						chunk_interval: test_params.chunk_interval(),
						accumulated_output_amount: test_params
							.expected_accumulated_amount(next_chunk),
					}
				);
			}

			// If its the first chunk of a ccm swap, we expect the gas swap to be scheduled
			if chunk == 1 && test_params.is_ccm {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: 3,
						input_amount: GAS_BUDGET,
						execute_at,
						swap_type: SwapType::CcmGas,
						..
					}) if *execute_at == test_params.expected_execution_block(chunk) + SWAP_DELAY_BLOCKS as u64
				);
				assert_eq!(
					get_ccm_gas_state(SWAP_REQUEST_ID),
					GasSwapState::Scheduled { gas_swap_id: 3 }
				);
			}

			test_params
		})
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

#[track_caller]
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
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;

	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	const TOTAL_OUTPUT_AMOUNT: AssetAmount = CHUNK_OUTPUT * 2;

	new_test_ext()
		.setup(DcaTestParams::new(2, CHUNK_INTERVAL))
		.expect_chunk_execution(1)
		.expect_chunk_execution(2)
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			// Check that all the events where emitted
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
	const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const INPUT_AMOUNT_AFTER_FEE: AssetAmount = INPUT_AMOUNT - BROKER_FEE;
	const EGRESS_AMOUNT: AssetAmount = INPUT_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	new_test_ext()
		.setup(DcaTestParams::new(1, CHUNK_INTERVAL))
		.expect_chunk_execution(1)
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
		.setup(DcaTestParams::new(NUMBER_OF_CHUNKS, CHUNK_INTERVAL).with_refund_params(
			TestRefundParams {
				// Allow for exactly 1 retry
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				// This ensures the swap is refunded:
				min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
			},
		))
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
					status: DcaStatus::ChunkScheduled(1),
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
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: INPUT_ASSET,
					amount: INPUT_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
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
		.setup(DcaTestParams::new(NUMBER_OF_CHUNKS, CHUNK_INTERVAL).with_refund_params(
			TestRefundParams {
				// Allow for one retry for good measure:
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				// This ensures the swap is refunded:
				min_output: INPUT_AMOUNT,
			},
		))
		.expect_chunk_execution(1)
		.then_execute_with(|_| {
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(2),
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
					status: DcaStatus::ChunkScheduled(2),
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
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
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
		.setup(DcaTestParams::new(NUMBER_OF_CHUNKS, CHUNK_INTERVAL).with_refund_params(
			TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				min_output: CHUNK_OUTPUT,
			},
		))
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
					status: DcaStatus::ChunkScheduled(1),
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
					status: DcaStatus::ChunkScheduled(2),
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

mod ccm_tests {

	use super::*;

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

	const GAS_SWAP_ID: SwapId = 3;

	#[test]
	fn dca_with_ccm_happy_path() {
		new_test_ext()
			.setup(DcaTestParams::new(2, CHUNK_INTERVAL).is_ccm())
			.expect_chunk_execution(1)
			.then_execute_at_block(GAS_BLOCK, |params| params)
			.then_execute_with(|params| {
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
						status: DcaStatus::ChunkScheduled(2),
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

				params
			})
			.expect_chunk_execution(2)
			.then_execute_with(|_| {
				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID
					}),
				);

				ccm::assert_ccm_egressed(
					OUTPUT_ASSET,
					CHUNK_OUTPUT * 2,
					GAS_BUDGET * DEFAULT_SWAP_RATE,
				)
			});
	}

	#[test]
	fn dca_with_ccm_full_refund() {
		new_test_ext()
			.setup(DcaTestParams::new(2, CHUNK_INTERVAL).is_ccm().with_refund_params(
				TestRefundParams {
					retry_duration: 0,
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE,
				},
			))
			.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
			.then_execute_with(|_| {
				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::RefundEgressScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						// Note that gas is refunded too:
						amount: INPUT_AMOUNT,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID
					}),
				);
			});
	}

	#[test]
	fn dca_with_ccm_partial_refund() {
		const PRINCIPAL_AMOUNT: AssetAmount = INPUT_AMOUNT - GAS_BUDGET;

		const CHUNK_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT / 2;

		new_test_ext()
			.setup(DcaTestParams::new(2, CHUNK_INTERVAL).is_ccm().with_refund_params(
				TestRefundParams {
					retry_duration: 0,
					// NOTE: divide by 2 to ensure swap succeeds even in presence of broker fees
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE / 2,
				},
			))
			.expect_chunk_execution(1)
			.then_execute_at_block(GAS_BLOCK, |_| {})
			.then_execute_with(|_| {
				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: GAS_SWAP_ID, .. }),
				);
			})
			.then_execute_at_block(CHUNK_2_BLOCK, |_| {
				SwapRate::set(DEFAULT_SWAP_RATE as f64 / 2f64);
			})
			.then_execute_with(|_| {
				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				ccm::assert_ccm_egressed(
					OUTPUT_ASSET,
					CHUNK_OUTPUT,
					GAS_BUDGET * DEFAULT_SWAP_RATE,
				);

				assert_event_sequence!(
					Test,
					// Only one chunk is refunded (does not include the first chunk and gas):
					RuntimeEvent::Swapping(Event::RefundEgressScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						amount: CHUNK_AMOUNT,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapEgressScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						amount: CHUNK_OUTPUT,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID
					}),
				);
			});
	}

	#[test]
	fn dca_with_ccm_partial_refund_gas_delayed() {
		// This ensures that gas and chunk 2 are scheduled for the same block:
		const CHUNK_INTERVAL: u32 = SWAP_DELAY_BLOCKS;
		const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

		const PRINCIPAL_AMOUNT: AssetAmount = INPUT_AMOUNT - GAS_BUDGET;

		const CHUNK_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT / 2;

		const NEW_SWAP_RATE: u128 = DEFAULT_SWAP_RATE / 2;

		new_test_ext()
			.setup(DcaTestParams::new(2, CHUNK_INTERVAL).is_ccm().with_refund_params(
				TestRefundParams {
					retry_duration: 0,
					// NOTE: divide by 2 to ensure swap succeeds even in presence of broker fees
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE / 2,
				},
			))
			.expect_chunk_execution(1)
			.then_execute_with(|_| {
				assert_eq!(
					get_dca_state(SWAP_REQUEST_ID),
					DcaState {
						status: DcaStatus::ChunkScheduled(2),
						remaining_input_amount: 0,
						remaining_chunks: 0,
						chunk_interval: CHUNK_INTERVAL,
						accumulated_output_amount: CHUNK_OUTPUT
					}
				);
			})
			.then_execute_at_block(CHUNK_2_BLOCK, |_| {
				SwapRate::set(NEW_SWAP_RATE as f64);
			})
			.then_execute_with(|_| {
				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				ccm::assert_ccm_egressed(OUTPUT_ASSET, CHUNK_OUTPUT, GAS_BUDGET * NEW_SWAP_RATE);

				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: GAS_SWAP_ID,
						..
					}),
					// Only one chunk is refunded (does not include the first chunk and gas):
					RuntimeEvent::Swapping(Event::RefundEgressScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						amount: CHUNK_AMOUNT,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapEgressScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						amount: CHUNK_OUTPUT,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapRequestCompleted {
						swap_request_id: SWAP_REQUEST_ID
					}),
				);
			});
	}
}
