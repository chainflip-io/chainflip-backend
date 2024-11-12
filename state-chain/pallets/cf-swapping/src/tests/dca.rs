use frame_support::assert_err;

use super::*;

const CHUNK_INTERVAL: u32 = 3;

#[track_caller]
fn setup_dca_swap(
	number_of_chunks: u32,
	chunk_interval: u32,
	refund_params: Option<TestRefundParams>,
) {
	// Sanity check that the test started at the correct block
	assert_eq!(System::block_number(), INIT_BLOCK);

	// Start the dca swap
	insert_swaps(&[TestSwapParams::new(
		Some(DcaParameters { number_of_chunks, chunk_interval }),
		refund_params,
		false,
	)]);

	// Check that the swap request was received;
	assert_has_matching_event!(
		Test,
		RuntimeEvent::Swapping(Event::SwapRequested {
			swap_request_id: SWAP_REQUEST_ID,
			input_amount: INPUT_AMOUNT,
			dca_parameters,
			..
		}) if dca_parameters == &Some(DcaParameters { number_of_chunks, chunk_interval })
	);

	// Check that the first chunk was scheduled
	let chunk_amount = INPUT_AMOUNT / number_of_chunks as u128;
	assert_has_matching_event!(
		Test,
		RuntimeEvent::Swapping(Event::SwapScheduled {
			swap_request_id: SWAP_REQUEST_ID,
			swap_id: SwapId(1),
			input_amount,
			execute_at,
			..
		}) if *input_amount == chunk_amount
			&& *execute_at == INIT_BLOCK + SWAP_DELAY_BLOCKS as u64
	);

	// Check the DCA state is correct
	assert_eq!(
		get_dca_state(SWAP_REQUEST_ID),
		DcaState {
			status: DcaStatus::ChunkScheduled(1.into()),
			remaining_input_amount: INPUT_AMOUNT - chunk_amount,
			remaining_chunks: number_of_chunks - 1,
			chunk_interval,
			accumulated_output_amount: 0,
		}
	);
}

#[track_caller]
fn assert_chunk_1_executed(number_of_chunks: u32) {
	let chunk_amount = INPUT_AMOUNT / number_of_chunks as u128;
	let chunk_amount_after_fee = chunk_amount - (chunk_amount * BROKER_FEE_BPS as u128 / 10_000);

	assert_has_matching_event!(
		Test,
		RuntimeEvent::Swapping(Event::SwapExecuted {
			swap_request_id: SWAP_REQUEST_ID,
			swap_id: SwapId(1),
			input_amount,
			output_amount,
			..
		}) if *input_amount == chunk_amount_after_fee && *output_amount == chunk_amount_after_fee * DEFAULT_SWAP_RATE
	);

	// Second chunk should be scheduled 2 blocks after the first is executed:
	assert_has_matching_event!(
		Test,
		RuntimeEvent::Swapping(Event::SwapScheduled {
			swap_request_id: SWAP_REQUEST_ID,
			swap_id: SwapId(2),
			input_amount,
			execute_at,
			..
		}) if *execute_at == System::block_number() + CHUNK_INTERVAL as u64 && *input_amount == chunk_amount
	);

	assert_eq!(
		get_dca_state(SWAP_REQUEST_ID),
		DcaState {
			status: DcaStatus::ChunkScheduled(2.into()),
			remaining_input_amount: INPUT_AMOUNT - (chunk_amount * 2),
			remaining_chunks: number_of_chunks - 2,
			chunk_interval: CHUNK_INTERVAL,
			accumulated_output_amount: chunk_amount_after_fee * DEFAULT_SWAP_RATE
		}
	);
}

#[track_caller]
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
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

	const NUMBER_OF_CHUNKS: u32 = 2;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;

	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	const TOTAL_OUTPUT_AMOUNT: AssetAmount = CHUNK_OUTPUT * 2;

	new_test_ext()
		.execute_with(|| {
			setup_dca_swap(NUMBER_OF_CHUNKS, CHUNK_INTERVAL, None);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_chunk_1_executed(NUMBER_OF_CHUNKS);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
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
			setup_dca_swap(1, CHUNK_INTERVAL, None);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
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
			setup_dca_swap(
				NUMBER_OF_CHUNKS,
				CHUNK_INTERVAL,
				Some(TestRefundParams {
					// Allow for exactly 1 retry
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					// This ensures the swap is refunded:
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
				}),
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(1),
					execute_at: REFUND_BLOCK
				})
			);

			// Note that there is no change to the DCA state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(1.into()),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0
				}
			);
		})
		.then_process_blocks_until_block(REFUND_BLOCK)
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
		.execute_with(|| {
			setup_dca_swap(
				NUMBER_OF_CHUNKS,
				CHUNK_INTERVAL,
				Some(TestRefundParams {
					// Allow for one retry for good measure:
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: INPUT_AMOUNT,
				}),
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_chunk_1_executed(NUMBER_OF_CHUNKS);
		})
		.then_execute_at_block(CHUNK_2_BLOCK, |_| {
			// Adjusting the swap rate, so that the second chunk fails due to FoK:
			SwapRate::set(0.5);
		})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(2),
					execute_at: CHUNK_2_RESCHEDULED_AT_BLOCK
				})
			);

			// Note that there is no change to the DCA state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(2.into()),
					remaining_input_amount: REFUNDED_AMOUNT - CHUNK_AMOUNT,
					remaining_chunks: 2,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);
		})
		.then_process_blocks_until_block(CHUNK_2_RESCHEDULED_AT_BLOCK)
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
		.execute_with(|| {
			setup_dca_swap(
				NUMBER_OF_CHUNKS,
				CHUNK_INTERVAL,
				Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: CHUNK_OUTPUT,
				}),
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
					swap_id: SwapId(1),
					execute_at: CHUNK_1_RETRY_BLOCK,
					..
				})
			);

			// No change:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(1.into()),
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
					swap_id: SwapId(1),
					input_amount: CHUNK_AMOUNT_AFTER_FEE,
					output_amount: CHUNK_OUTPUT,
					broker_fee: CHUNK_BROKER_FEE,
					..
				}),
				// Second chunk should be scheduled 2 blocks after the first is executed:
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(2.into()),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT
				}
			);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
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
fn can_handle_dca_chunk_size_of_zero() {
	// The input amount is smaller than the number of chunks, so the chunk size will round down to 0
	const INPUT_AMOUNT: AssetAmount = 1;
	const NUMBER_OF_CHUNKS: u32 = 3;
	const ZERO_CHUNK_AMOUNT: AssetAmount = 0;
	// Even though the chunk size is 0, the end output should still the full amount * swap rate.
	// Note that the broker fee is 0 in this case because the input is too small.
	const OUTPUT_AMOUNT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE;

	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;
	const CHUNK_3_BLOCK: u64 = CHUNK_2_BLOCK + CHUNK_INTERVAL as u64;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			// Start the dca swap
			let dca_params = DcaParameters {
				number_of_chunks: NUMBER_OF_CHUNKS,
				chunk_interval: CHUNK_INTERVAL,
			};
			let swap_params = TestSwapParams {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				refund_params: None,
				dca_params: Some(dca_params.clone()),
				output_address: (*EVM_OUTPUT_ADDRESS).clone(),
				is_ccm: false,
			};
			insert_swaps(&[swap_params]);

			// Check that the swap request was received;
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					dca_parameters,
					..
				}) if dca_parameters == &Some(dca_params.clone())
			);

			// Check that the first chunk was scheduled
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
					input_amount,
					..
					// All chunks should be 0 amount except the last one
				}) if *input_amount == ZERO_CHUNK_AMOUNT
			);

			// Check the DCA state is correct
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(1.into()),
					// Still the full amount remaining because the first chunk is 0
					remaining_input_amount: INPUT_AMOUNT,
					remaining_chunks: NUMBER_OF_CHUNKS - 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0,
				}
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
					input_amount,
					output_amount,
					..
					// The first chunk should 0 in and out
				}) if *input_amount == ZERO_CHUNK_AMOUNT && *output_amount == ZERO_CHUNK_AMOUNT
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input_amount,
					..
					// All chunks should be 0 amount except the last one
				}) if *input_amount == ZERO_CHUNK_AMOUNT
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					status: DcaStatus::ChunkScheduled(2.into()),
					remaining_input_amount: INPUT_AMOUNT,
					remaining_chunks: NUMBER_OF_CHUNKS - 2,
					chunk_interval: CHUNK_INTERVAL,
					// Should still be 0
					accumulated_output_amount: 0,
				}
			);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_process_blocks_until_block(CHUNK_3_BLOCK)
		.then_execute_with(|_| {
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(3),
					input_amount,
					output_amount,
					..
					// The last chunk should be the full amount
				}) if *input_amount == INPUT_AMOUNT && *output_amount == OUTPUT_AMOUNT
			);

			assert_has_matching_event!(
				Test,
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

	const GAS_SWAP_ID: SwapId = SwapId(3);

	#[track_caller]
	fn setup_ccm_dca_swap(
		number_of_chunks: u32,
		chunk_interval: u32,
		refund_params: Option<TestRefundParams>,
	) {
		// Sanity check that the test started at the correct block
		assert_eq!(System::block_number(), INIT_BLOCK);

		insert_swaps(&[TestSwapParams::new(
			Some(DcaParameters { number_of_chunks, chunk_interval }),
			refund_params,
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
				swap_id: SwapId(1),
				input_amount: CHUNK_AMOUNT,
				execute_at: CHUNK_1_BLOCK,
				..
			})
		);

		assert_eq!(
			get_dca_state(SWAP_REQUEST_ID),
			DcaState {
				status: DcaStatus::ChunkScheduled(1.into()),
				remaining_input_amount: CHUNK_AMOUNT,
				remaining_chunks: 1,
				chunk_interval,
				accumulated_output_amount: 0
			}
		);

		assert_eq!(
			get_ccm_gas_state(SWAP_REQUEST_ID),
			GasSwapState::ToBeScheduled { gas_budget: GAS_BUDGET, other_gas_asset: OUTPUT_ASSET }
		);
	}

	#[track_caller]
	fn assert_first_ccm_chunk_successful() {
		// Once the first chunk is successfully executed, gas swap should be
		// scheduled together with the second chunk:
		assert_event_sequence!(
			Test,
			RuntimeEvent::Swapping(Event::SwapExecuted {
				swap_request_id: SWAP_REQUEST_ID,
				swap_id: SwapId(1),
				input_amount: CHUNK_AMOUNT_AFTER_FEE,
				output_amount: CHUNK_OUTPUT,
				..
			}),
			RuntimeEvent::Swapping(Event::SwapScheduled {
				swap_request_id: SWAP_REQUEST_ID,
				swap_id: SwapId(2),
				input_amount: CHUNK_AMOUNT,
				execute_at: CHUNK_2_BLOCK,
				swap_type: SwapType::CcmPrincipal,
				..
			}),
			RuntimeEvent::Swapping(Event::SwapScheduled {
				swap_request_id: SWAP_REQUEST_ID,
				swap_id: GAS_SWAP_ID,
				input_amount: GAS_BUDGET,
				execute_at: GAS_BLOCK,
				swap_type: SwapType::CcmGas,
				..
			}),
		);

		assert_eq!(
			get_dca_state(SWAP_REQUEST_ID),
			DcaState {
				status: DcaStatus::ChunkScheduled(2.into()),
				remaining_input_amount: 0,
				remaining_chunks: 0,
				chunk_interval: CHUNK_INTERVAL,
				accumulated_output_amount: CHUNK_OUTPUT
			}
		);

		assert_eq!(
			get_ccm_gas_state(SWAP_REQUEST_ID),
			GasSwapState::Scheduled { gas_swap_id: GAS_SWAP_ID }
		);
	}

	#[test]
	fn dca_with_ccm_happy_path() {
		new_test_ext()
			.execute_with(|| {
				setup_ccm_dca_swap(2, CHUNK_INTERVAL, None);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
			.then_execute_with(|_| {
				assert_first_ccm_chunk_successful();
			})
			.then_process_blocks_until_block(GAS_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: SwapId(3),
						input_amount: GAS_BUDGET,
						output_amount,
						..
					}) if *output_amount == GAS_BUDGET * DEFAULT_SWAP_RATE,
				);

				// Gas swap has no effect on the DCA principal state:
				assert_eq!(
					get_dca_state(SWAP_REQUEST_ID),
					DcaState {
						status: DcaStatus::ChunkScheduled(2.into()),
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
			.then_process_blocks_until_block(CHUNK_2_BLOCK)
			.then_execute_with(|_| {
				assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: SwapId(2),
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
			.execute_with(|| {
				setup_ccm_dca_swap(
					2,
					CHUNK_INTERVAL,
					Some(TestRefundParams {
						retry_duration: 0,
						min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE,
					}),
				);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
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
		const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		const PRINCIPAL_AMOUNT: AssetAmount = INPUT_AMOUNT - GAS_BUDGET;

		const CHUNK_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT / 2;

		new_test_ext()
			.execute_with(|| {
				setup_ccm_dca_swap(
					2,
					CHUNK_INTERVAL,
					Some(TestRefundParams {
						retry_duration: 0,
						// NOTE: divide by 2 to ensure swap succeeds even in presence of broker fees
						min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE / 2,
					}),
				);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
			.then_execute_with(|_| {
				assert_first_ccm_chunk_successful();
			})
			.then_process_blocks_until_block(GAS_BLOCK)
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
			.execute_with(|| {
				setup_ccm_dca_swap(
					2,
					CHUNK_INTERVAL,
					Some(TestRefundParams {
						retry_duration: 0,
						// NOTE: divide by 2 to ensure swap succeeds even in presence of broker fees
						min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE / 2,
					}),
				);

				assert_eq!(
					get_ccm_gas_state(SWAP_REQUEST_ID),
					GasSwapState::ToBeScheduled {
						gas_budget: GAS_BUDGET,
						other_gas_asset: OUTPUT_ASSET
					}
				);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
			.then_execute_with(|_| {
				assert_event_sequence!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: SwapId(1),
						input_amount: CHUNK_AMOUNT_AFTER_FEE,
						output_amount: CHUNK_OUTPUT,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: SwapId(2),
						input_amount: CHUNK_AMOUNT,
						execute_at: CHUNK_2_BLOCK,
						swap_type: SwapType::CcmPrincipal,
						..
					}),
					RuntimeEvent::Swapping(Event::SwapScheduled {
						swap_request_id: SWAP_REQUEST_ID,
						swap_id: GAS_SWAP_ID,
						input_amount: GAS_BUDGET,
						execute_at: GAS_BLOCK,
						swap_type: SwapType::CcmGas,
						..
					}),
				);

				assert_eq!(
					get_dca_state(SWAP_REQUEST_ID),
					DcaState {
						status: DcaStatus::ChunkScheduled(2.into()),
						remaining_input_amount: 0,
						remaining_chunks: 0,
						chunk_interval: CHUNK_INTERVAL,
						accumulated_output_amount: CHUNK_OUTPUT
					}
				);

				assert_eq!(
					get_ccm_gas_state(SWAP_REQUEST_ID),
					GasSwapState::Scheduled { gas_swap_id: GAS_SWAP_ID }
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

#[test]
fn test_minimum_chunk_size() {
	#[track_caller]
	fn set_and_test_chunk_size(
		asset_amount: AssetAmount,
		number_of_chunks: u32,
		expected_number_of_chunks: u32,
		minimum_chunk_size: AssetAmount,
	) {
		// Update the minimum chunk size
		assert_ok!(Swapping::update_pallet_config(
			OriginTrait::root(),
			vec![PalletConfigUpdate::SetMinimumChunkSize {
				asset: Asset::Eth,
				size: minimum_chunk_size
			},]
			.try_into()
			.unwrap()
		));

		// Init a swap, this is where the minimum chunk size will kick in
		let dca_params = DcaParameters { number_of_chunks, chunk_interval: CHUNK_INTERVAL };
		let expected_swap_request_id = Swapping::init_swap_request(
			Asset::Eth,
			asset_amount,
			Asset::Btc,
			SwapRequestType::Regular { output_address: ForeignChainAddress::Eth([1; 20].into()) },
			vec![].try_into().unwrap(),
			None,
			Some(dca_params),
			SwapOrigin::Vault { tx_id: TransactionInIdForAnyChain::ByteHash(H256::default()) },
		);

		// Check that the swap was initiated with the updated number of chunks
		let expected_dca_params = DcaParameters {
			number_of_chunks: expected_number_of_chunks,
			chunk_interval: CHUNK_INTERVAL,
		};
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapRequested {swap_request_id, dca_parameters, .. })
				if dca_parameters == &Some(expected_dca_params.clone()) && *swap_request_id == expected_swap_request_id
		);
	}

	new_test_ext().execute_with(|| {
		set_and_test_chunk_size(100, 10, 10, 9);
		set_and_test_chunk_size(100, 10, 10, 10);
		set_and_test_chunk_size(100, 10, 9, 11);
		set_and_test_chunk_size(1, 10, 1, 10);
		set_and_test_chunk_size(1, 1000, 1000, 0);
	});
}

#[test]
fn test_dca_parameter_validation() {
	use cf_traits::SwapLimitsProvider;

	fn validate_dca_params(
		number_of_chunks: u32,
		chunk_interval: u32,
	) -> Result<(), DispatchError> {
		Swapping::validate_dca_params(&DcaParameters { number_of_chunks, chunk_interval })
	}

	new_test_ext().execute_with(|| {
		const MIN_CHUNK_INTERVAL: u32 = SWAP_DELAY_BLOCKS;
		let max_swap_request_duration_blocks = MaxSwapRequestDurationBlocks::<Test>::get();

		// Trivially ok
		assert_ok!(validate_dca_params(1, MIN_CHUNK_INTERVAL));
		assert_ok!(validate_dca_params(2, MIN_CHUNK_INTERVAL));

		// Equal to the limit
		assert_ok!(validate_dca_params(
			(max_swap_request_duration_blocks / MIN_CHUNK_INTERVAL) + 1,
			MIN_CHUNK_INTERVAL
		));
		assert_ok!(validate_dca_params(2, max_swap_request_duration_blocks));

		// Limit is ignored because there is only 1 chunk
		assert_ok!(validate_dca_params(1, max_swap_request_duration_blocks + 100));
		assert_ok!(validate_dca_params(1, 0));

		// Exceeding limit
		assert_err!(
			validate_dca_params(
				(max_swap_request_duration_blocks / MIN_CHUNK_INTERVAL) + 2,
				MIN_CHUNK_INTERVAL
			),
			DispatchError::from(crate::Error::<Test>::SwapRequestDurationTooLong)
		);
		assert_err!(
			validate_dca_params(2, max_swap_request_duration_blocks + 1),
			DispatchError::from(crate::Error::<Test>::SwapRequestDurationTooLong)
		);

		// Below the minimum
		assert_err!(
			validate_dca_params(10, 1),
			DispatchError::from(crate::Error::<Test>::ChunkIntervalTooLow)
		);
		assert_err!(
			validate_dca_params(0, MIN_CHUNK_INTERVAL),
			DispatchError::from(crate::Error::<Test>::ZeroNumberOfChunksNotAllowed)
		);
	});
}
