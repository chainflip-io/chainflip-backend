// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use frame_support::assert_err;

use super::*;

const CHUNK_INTERVAL: u32 = 3;

#[track_caller]
fn setup_dca_swap(
	number_of_chunks: u32,
	chunk_interval: u32,
	refund_params: Option<TestRefundParams>,
	is_ccm: bool,
) {
	// Sanity check that the test started at the correct block
	assert_eq!(System::block_number(), INIT_BLOCK);

	// Start the dca swap
	insert_swaps(&[TestSwapParams::new(
		Some(DcaParameters { number_of_chunks, chunk_interval }),
		refund_params,
		is_ccm,
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
			scheduled_chunks: BTreeSet::from([(1.into())]),
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
			scheduled_chunks: BTreeSet::from([(2.into())]),
			remaining_input_amount: INPUT_AMOUNT - (chunk_amount * 2),
			remaining_chunks: number_of_chunks - 2,
			chunk_interval: CHUNK_INTERVAL,
			accumulated_output_amount: chunk_amount_after_fee * DEFAULT_SWAP_RATE,
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

#[test]
fn dca_happy_path_ccm() {
	dca_happy_path(true);
}

#[test]
fn dca_happy_path_no_ccm() {
	dca_happy_path(false);
}

fn dca_happy_path(is_ccm: bool) {
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
			setup_dca_swap(NUMBER_OF_CHUNKS, CHUNK_INTERVAL, None, is_ccm);
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

#[test]
fn dca_single_chunk_ccm() {
	dca_single_chunk(true);
}

#[test]
fn dca_single_chunk_no_ccm() {
	dca_single_chunk(false);
}

/// Test that DCA with 1 chunk behaves like a regular swap
fn dca_single_chunk(is_ccm: bool) {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const INPUT_AMOUNT_AFTER_FEE: AssetAmount = INPUT_AMOUNT - BROKER_FEE;
	const EGRESS_AMOUNT: AssetAmount = INPUT_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			setup_dca_swap(1, CHUNK_INTERVAL, None, is_ccm);
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
fn dca_with_fok_full_refund_ccm() {
	dca_with_fok_full_refund(true);
}

#[test]
fn dca_with_fok_full_refund_no_ccm() {
	dca_with_fok_full_refund(false);
}

fn dca_with_fok_full_refund(is_ccm: bool) {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const NUMBER_OF_CHUNKS: u32 = 2;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;

	// Allow for one retry for good measure:
	const REFUND_BLOCK: u64 = CHUNK_1_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);
	const REFUND_FEE: AssetAmount = 10;
	const REFUNDED_AMOUNT: AssetAmount = INPUT_AMOUNT - REFUND_FEE;

	new_test_ext()
		.execute_with(|| {
			// Turn on the network fee minimum so we can check the refund fee works correctly
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::zero(),
				minimum: REFUND_FEE,
			});

			setup_dca_swap(
				NUMBER_OF_CHUNKS,
				CHUNK_INTERVAL,
				Some(TestRefundParams {
					// Allow for exactly 1 retry
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					// This ensures the swap is refunded:
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1,
				}),
				is_ccm,
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(1),
					execute_at: REFUND_BLOCK,
					reason: SwapFailureReason::MinPriceViolation,
				})
			);

			// Note that there is no change to the DCA state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunks: BTreeSet::from([(1.into())]),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0,
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
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(1),
					reason: SwapFailureReason::MinPriceViolation
				}),
				RuntimeEvent::Swapping(Event::SwapRequested {
					input_asset: INPUT_ASSET,
					input_amount: REFUND_FEE,
					output_asset: Asset::Flip,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled { input_amount: REFUND_FEE, .. }),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: INPUT_ASSET,
					amount: REFUNDED_AMOUNT,
					refund_fee: REFUND_FEE,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);
		});
}

#[test]
fn dca_with_fok_partial_refund_ccm() {
	dca_with_fok_partial_refund(true);
}

#[test]
fn dca_with_fok_partial_refund_no_ccm() {
	dca_with_fok_partial_refund(false);
}

fn dca_with_fok_partial_refund(is_ccm: bool) {
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;
	const CHUNK_2_RESCHEDULED_AT_BLOCK: u64 =
		CHUNK_2_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	const NUMBER_OF_CHUNKS: u32 = 4;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;
	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;

	const REFUND_FEE: AssetAmount = 10;
	// The test will be set up as to execute one chunk only and refund the rest
	const REFUNDED_AMOUNT: AssetAmount = INPUT_AMOUNT - CHUNK_AMOUNT - REFUND_FEE;

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
				is_ccm,
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
					execute_at: CHUNK_2_RESCHEDULED_AT_BLOCK,
					reason: SwapFailureReason::MinPriceViolation,
				})
			);

			// Note that there is no change to the DCA state:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunks: BTreeSet::from([(2.into())]),
					remaining_input_amount: INPUT_AMOUNT - CHUNK_AMOUNT * 2,
					remaining_chunks: 2,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT,
				}
			);

			// Now turn on the network fee minimum so we can check the refund fee works correctly
			// without needing to take it into account on the other chunks.
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::zero(),
				minimum: REFUND_FEE,
			});
		})
		.then_process_blocks_until_block(CHUNK_2_RESCHEDULED_AT_BLOCK)
		.then_execute_with(|_| {
			// The swap will fail again, but this time it will reach expiry,
			// resulting in a refund of the remaining amount and egress of the
			// already executed amount.
			assert_eq!(SwapRequests::<Test>::get(SWAP_REQUEST_ID), None);

			if is_ccm {
				ccm::assert_ccm_egressed(Asset::Eth, CHUNK_OUTPUT, GAS_BUDGET);
			}

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(2),
					reason: SwapFailureReason::MinPriceViolation
				}),
				RuntimeEvent::Swapping(Event::SwapRequested {
					input_asset: INPUT_ASSET,
					input_amount: REFUND_FEE,
					output_asset: Asset::Flip,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled { input_amount: REFUND_FEE, .. }),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: INPUT_ASSET,
					amount: REFUNDED_AMOUNT,
					refund_fee: REFUND_FEE,
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
fn dca_with_fok_fully_executed_ccm() {
	dca_with_fok_fully_executed(true);
}

#[test]
fn dca_with_fok_fully_executed_no_ccm() {
	dca_with_fok_fully_executed(false);
}

fn dca_with_fok_fully_executed(is_ccm: bool) {
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
				is_ccm,
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
					reason: SwapFailureReason::MinPriceViolation,
				})
			);

			// No change:
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunks: BTreeSet::from([(1.into())]),
					remaining_input_amount: CHUNK_AMOUNT,
					remaining_chunks: 1,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: 0,
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
					scheduled_chunks: BTreeSet::from([(2.into())]),
					remaining_input_amount: 0,
					remaining_chunks: 0,
					chunk_interval: CHUNK_INTERVAL,
					accumulated_output_amount: CHUNK_OUTPUT,
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
fn can_handle_dca_chunk_size_of_zero_ccm() {
	can_handle_dca_chunk_size_of_zero(true);
}

#[test]
fn can_handle_dca_chunk_size_of_zero_no_ccm() {
	can_handle_dca_chunk_size_of_zero(false);
}

fn can_handle_dca_chunk_size_of_zero(is_ccm: bool) {
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
				price_limits_and_expiry: None,
				dca_params: Some(dca_params.clone()),
				output_address: (*EVM_OUTPUT_ADDRESS).clone(),
				is_ccm,
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
					// All chunks should be 0 amount except the last one
					input_amount: ZERO_CHUNK_AMOUNT,
					..
				})
			);

			// Check the DCA state is correct
			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunks: BTreeSet::from([(1.into())]),
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
					// The first chunk should 0 in and out
					input_amount: ZERO_CHUNK_AMOUNT,
					output_amount: ZERO_CHUNK_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					// All chunks should be 0 amount except the last one
					input_amount: ZERO_CHUNK_AMOUNT,
					..
				})
			);

			assert_eq!(
				get_dca_state(SWAP_REQUEST_ID),
				DcaState {
					scheduled_chunks: BTreeSet::from([(2.into())]),
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
					// The last chunk should be the full amount
					input_amount: INPUT_AMOUNT,
					output_amount: OUTPUT_AMOUNT,
					..
				})
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);
		});
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
			SwapRequestType::Regular {
				output_action: SwapOutputAction::Egress {
					output_address: ForeignChainAddress::Eth([1; 20].into()),
					ccm_deposit_metadata: None,
				},
			},
			vec![].try_into().unwrap(),
			None,
			Some(dca_params),
			SwapOrigin::Vault {
				tx_id: TransactionInIdForAnyChain::Evm(H256::default()),
				broker_id: Some(BROKER),
			},
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
	use cf_traits::SwapParameterValidation;

	fn validate_dca_params(
		number_of_chunks: u32,
		chunk_interval: u32,
	) -> Result<(), DispatchError> {
		Swapping::validate_dca_params(&DcaParameters { number_of_chunks, chunk_interval })
	}

	new_test_ext().execute_with(|| {
		const MIN_CHUNK_INTERVAL: u32 = 1;
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
			validate_dca_params(10, 0),
			DispatchError::from(crate::Error::<Test>::ChunkIntervalTooLow)
		);
		assert_err!(
			validate_dca_params(0, MIN_CHUNK_INTERVAL),
			DispatchError::from(crate::Error::<Test>::ZeroNumberOfChunksNotAllowed)
		);
	});
}

#[test]
fn dca_with_one_block_interval() {
	const ONE_BLOCK_CHUNK_INTERVAL: u32 = 1;
	const NUMBER_OF_CHUNKS: u32 = 4;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + ONE_BLOCK_CHUNK_INTERVAL as u64;
	const CHUNK_3_BLOCK: u64 = CHUNK_2_BLOCK + ONE_BLOCK_CHUNK_INTERVAL as u64;
	const CHUNK_4_BLOCK: u64 = CHUNK_3_BLOCK + ONE_BLOCK_CHUNK_INTERVAL as u64;

	assert_eq!(
		SWAP_DELAY_BLOCKS, 2,
		"Tests and code in init_swap_request assumes the swap delay is 2 blocks,
			so only a max of 2 chunks can be scheduled at a time."
	);

	new_test_ext()
		.execute_with(|| {
			insert_swaps(&[TestSwapParams::new(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: ONE_BLOCK_CHUNK_INTERVAL,
				}),
				None,  // no refund params
				false, // no ccm
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					dca_parameters: Some(DcaParameters {
						number_of_chunks: NUMBER_OF_CHUNKS,
						chunk_interval: ONE_BLOCK_CHUNK_INTERVAL
					}),
					..
				})
			);

			// 2 chunks should be scheduled at the same time with a 1 block interval
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
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
					..
				})
			);

			// Now the last chunk should be scheduled, but the execute_at should be 1 block after
			// chunk 2 (instead of 1 block after the just completed chunk 1).
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(3),
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_3_BLOCK,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_4_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);
		});
}

#[test]
fn dca_with_one_block_interval_fok() {
	const ONE_BLOCK_CHUNK_INTERVAL: u32 = 1;
	const NUMBER_OF_CHUNKS: u32 = 4;
	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / NUMBER_OF_CHUNKS as u128;
	const CHUNK_BROKER_FEE: AssetAmount = CHUNK_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
	const CHUNK_AMOUNT_AFTER_FEE: AssetAmount = CHUNK_AMOUNT - CHUNK_BROKER_FEE;
	const CHUNK_OUTPUT: AssetAmount = CHUNK_AMOUNT_AFTER_FEE * DEFAULT_SWAP_RATE;
	const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + ONE_BLOCK_CHUNK_INTERVAL as u64;
	const CHUNK_2_RESCHEDULED_AT_BLOCK: u64 =
		CHUNK_2_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);
	const CHUNK_3_BLOCK: u64 = CHUNK_2_BLOCK + ONE_BLOCK_CHUNK_INTERVAL as u64;
	const CHUNK_3_RESCHEDULED_AT_BLOCK: u64 =
		CHUNK_2_RESCHEDULED_AT_BLOCK + ONE_BLOCK_CHUNK_INTERVAL as u64;

	new_test_ext()
		.execute_with(|| {
			insert_swaps(&[TestSwapParams::new(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: ONE_BLOCK_CHUNK_INTERVAL,
				}),
				Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: CHUNK_OUTPUT,
				}),
				false, // no ccm
			)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SWAP_REQUEST_ID,
					input_amount: INPUT_AMOUNT,
					dca_parameters: Some(DcaParameters {
						number_of_chunks: NUMBER_OF_CHUNKS,
						chunk_interval: ONE_BLOCK_CHUNK_INTERVAL
					}),
					..
				})
			);

			// Both chunks should be scheduled at the same time with a 1 block interval
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
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(2),
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_2_BLOCK,
					..
				})
			);
		})
		.then_process_blocks_until_block(CHUNK_1_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(1),
					..
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: SwapId(3),
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_3_BLOCK,
					..
				})
			);

			// Make sure the swap queue is correct
			assert!(get_scheduled_swap_block(SwapId(1)).is_none());
			assert_eq!(get_scheduled_swap_block(SwapId(2)), Some(CHUNK_2_BLOCK));
			assert_eq!(get_scheduled_swap_block(SwapId(3)), Some(CHUNK_3_BLOCK));
			assert!(get_scheduled_swap_block(SwapId(4)).is_none());

			// Change the swap rate so the second chunk fails
			SwapRate::set((DEFAULT_SWAP_RATE / 10) as f64);
		})
		.then_process_blocks_until_block(CHUNK_2_BLOCK)
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(2),
					execute_at: CHUNK_2_RESCHEDULED_AT_BLOCK,
					reason: SwapFailureReason::MinPriceViolation,
				})
			);
			// The 3rd chunk should be rescheduled as well
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: SwapId(3),
					execute_at: CHUNK_3_RESCHEDULED_AT_BLOCK,
					reason: SwapFailureReason::PredecessorSwapFailure,
				})
			);

			// Make sure the old entry was removed from the swap queue and only the new one is there
			assert_eq!(get_scheduled_swap_block(SwapId(2)), Some(CHUNK_2_RESCHEDULED_AT_BLOCK));
			assert_eq!(get_scheduled_swap_block(SwapId(3)), Some(CHUNK_3_RESCHEDULED_AT_BLOCK));
		})
		.then_process_blocks_until_block(CHUNK_2_RESCHEDULED_AT_BLOCK)
		.then_execute_with(|_| {
			// Make sure that chunk 2 failing cancels chunk 3 that was already scheduled
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(2),
					reason: SwapFailureReason::MinPriceViolation
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(3),
					reason: SwapFailureReason::PredecessorSwapFailure,
				})
			);
			assert_swaps_queue_is_empty();

			// The refund amount should be for all 3 remaining chunks, including the canceled one.
			const REFUND_AMOUNT: AssetAmount = CHUNK_AMOUNT * 3;
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					amount: REFUND_AMOUNT,
					..
				})
			);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					amount: CHUNK_OUTPUT,
					..
				})
			);
		});
}
