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

const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;

fn fok_swap(refund_params: Option<TestRefundParams>, is_ccm: bool) -> TestSwapParams {
	TestSwapParams::new(None, refund_params, is_ccm)
}

#[track_caller]
fn assert_swaps_scheduled_for_block(swap_ids: &[SwapId], expected_block_number: u64) {
	for expected_swap_id in swap_ids {
		assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapScheduled { swap_id, execute_at: block_number, .. }) if swap_id == expected_swap_id && block_number == &expected_block_number,
		);
	}
}
#[test]
fn both_fok_and_regular_swaps_succeed_first_try_no_ccm() {
	both_fok_and_regular_swaps_succeed_first_try(false);
}

#[test]
fn both_fok_and_regular_swaps_succeed_first_try_ccm() {
	both_fok_and_regular_swaps_succeed_first_try(true);
}

fn both_fok_and_regular_swaps_succeed_first_try(is_ccm: bool) {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const REGULAR_SWAP_ID: SwapId = SwapId(1);
	const FOK_SWAP_ID: SwapId = SwapId(2);

	const REGULAR_REQUEST_ID: SwapRequestId = SwapRequestId(1);
	const FOK_REQUEST_ID: SwapRequestId = SwapRequestId(2);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			const REFUND_PARAMS: TestRefundParams = TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
				min_output: (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE,
			};

			let refund_parameters_encoded = REFUND_PARAMS.into_extended_params(INPUT_AMOUNT);

			insert_swaps(&vec![fok_swap(None, is_ccm), fok_swap(Some(REFUND_PARAMS), is_ccm)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: FOK_REQUEST_ID,
					price_limits_and_expiry,
					..
				}) if price_limits_and_expiry.as_ref() == Some(&refund_parameters_encoded),
			);

			assert_swaps_scheduled_for_block(
				&[REGULAR_SWAP_ID, FOK_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(SWAPS_SCHEDULED_FOR_BLOCK)
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: REGULAR_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: REGULAR_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: REGULAR_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: FOK_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_REQUEST_ID
				}),
			);
		});
}

#[test]
fn price_limit_is_respected_in_fok_swap_no_ccm() {
	price_limit_is_respected_in_fok_swap(false);
}

#[test]
fn price_limit_is_respected_in_fok_swap_ccm() {
	price_limit_is_respected_in_fok_swap(true);
}

fn price_limit_is_respected_in_fok_swap(is_ccm: bool) {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 =
		SWAPS_SCHEDULED_FOR_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;

	const EXPECTED_OUTPUT: AssetAmount = (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE;
	const HIGH_OUTPUT: AssetAmount = EXPECTED_OUTPUT + 2; // 2 higher because of rounding errors

	const REGULAR_SWAP_ID: SwapId = SwapId(1);
	const FOK_SWAP_1_ID: SwapId = SwapId(2);
	const FOK_SWAP_2_ID: SwapId = SwapId(3);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			insert_swaps(&vec![
				fok_swap(None, is_ccm),
				fok_swap(
					Some(TestRefundParams {
						retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
						min_output: HIGH_OUTPUT,
					}),
					is_ccm,
				),
				fok_swap(
					Some(TestRefundParams {
						retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
						min_output: EXPECTED_OUTPUT,
					}),
					is_ccm,
				),
			]);

			assert_swaps_scheduled_for_block(
				&[REGULAR_SWAP_ID, FOK_SWAP_1_ID, FOK_SWAP_2_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(3u64)
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), SWAPS_SCHEDULED_FOR_BLOCK);
			// Swap 2 should fail due to price limit and rescheduled for block
			// `SWAPS_SCHEDULED_FOR_BLOCK + SWAP_RETRY_DELAY_BLOCKS`, but swaps 1 and 3 should be
			// successful:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: REGULAR_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1)
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_2_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(3),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(3)
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_1_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
					reason: SwapFailureReason::MinPriceViolation,
				}),
			);

			assert_eq!(ScheduledSwaps::<Test>::get().len(), 1);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {
			// Changing the swap rate to allow the FoK swap to be executed
			SwapRate::set(HIGH_OUTPUT as f64 / (INPUT_AMOUNT - BROKER_FEE) as f64);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_1_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(2),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(2)
				}),
			);

			assert_swaps_queue_is_empty();
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_limit_no_ccm() {
	fok_swap_gets_refunded_due_to_price_limit(false);
}

#[test]
fn fok_swap_gets_refunded_due_to_price_limit_ccm() {
	fok_swap_gets_refunded_due_to_price_limit(true);
}

fn fok_swap_gets_refunded_due_to_price_limit(is_ccm: bool) {
	const FOK_SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(1);
	const OTHER_SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(2);

	const FOK_SWAP_ID: SwapId = SwapId(1);
	const OTHER_SWAP_ID: SwapId = SwapId(2);

	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 =
		SWAPS_SCHEDULED_FOR_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			// Min output for swap 1 is too high to be executed:
			const MIN_OUTPUT: AssetAmount = (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE + 2; // 2 higher because of rounding errors
			insert_swaps(&[fok_swap(
				Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: MIN_OUTPUT,
				}),
				is_ccm,
			)]);
			// However, swap 2 is non-FoK and should still be executed:
			insert_swaps(&[fok_swap(None, is_ccm)]);

			assert_swaps_scheduled_for_block(
				&[FOK_SWAP_ID, OTHER_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(SWAPS_SCHEDULED_FOR_BLOCK)
		.then_execute_with(|_| {
			// Swap 1 should fail here and rescheduled for a later block,
			// but swap 2 (without FoK parameters) should still be successful:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: OTHER_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: OTHER_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: OTHER_SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
					reason: SwapFailureReason::MinPriceViolation,
				}),
			);
		})
		.then_process_blocks_until_block(SWAP_RETRIED_AT_BLOCK)
		.then_execute_with(|_| {
			// Swap request should be removed in case of refund
			assert_eq!(SwapRequests::<Test>::get(FOK_SWAP_REQUEST_ID), None);
			// Swap should fail here (due to price limit) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(1),
					reason: SwapFailureReason::MinPriceViolation
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
			);
		});
}

#[test]
fn storage_state_rolls_back_on_fok_violation_no_ccm() {
	storage_state_rolls_back_on_fok_violation(false);
}

#[test]
fn storage_state_rolls_back_on_fok_violation_ccm() {
	storage_state_rolls_back_on_fok_violation(true);
}

fn storage_state_rolls_back_on_fok_violation(is_ccm: bool) {
	const FOK_SWAP_ID: SwapId = SwapId(1);
	const OTHER_SWAP_ID: SwapId = SwapId(2);

	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const EXPECTED_NETWORK_FEE_AMOUNT: AssetAmount = INPUT_AMOUNT / 100;

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::from_percent(1),
				minimum: 0,
			});

			MockSwappingApi::add_liquidity(INPUT_ASSET, 0);

			// This is about 2 times (ignoring fees) what the output will be, so will fail
			const MIN_OUTPUT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE * 2;
			insert_swaps(&[fok_swap(
				Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: MIN_OUTPUT,
				}),
				is_ccm,
			)]);
			// However, swap 2 is non-FoK and should still be executed:
			insert_swaps(&[fok_swap(None, is_ccm)]);

			assert_swaps_scheduled_for_block(
				&[FOK_SWAP_ID, OTHER_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_process_blocks_until_block(SWAPS_SCHEDULED_FOR_BLOCK)
		.then_execute_with(|_| {
			// Swap 1 should fail here and rescheduled for a later block,
			// but swap 2 (without FoK parameters) should still be successful:
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: OTHER_SWAP_ID, .. })
			);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: FOK_SWAP_ID, .. }),
			);

			// This ensures that storage from the initial failure was reverted (otherwise
			// we would see the network fee charged more than once)
			assert_eq!(CollectedNetworkFee::<Test>::get(), EXPECTED_NETWORK_FEE_AMOUNT);

			assert_eq!(
				MockSwappingApi::get_liquidity(&INPUT_ASSET),
				INPUT_AMOUNT - BROKER_FEE - EXPECTED_NETWORK_FEE_AMOUNT
			);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_impact_protection_no_ccm() {
	fok_swap_gets_refunded_due_to_price_impact_protection(false);
}

#[test]
fn fok_swap_gets_refunded_due_to_price_impact_protection_ccm() {
	fok_swap_gets_refunded_due_to_price_impact_protection(true);
}

fn fok_swap_gets_refunded_due_to_price_impact_protection(is_ccm: bool) {
	const FOK_SWAP_REQUEST_ID: SwapRequestId = SwapRequestId(1);
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 =
		SWAPS_SCHEDULED_FOR_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

	const FOK_SWAP_ID: SwapId = SwapId(1);
	const REGULAR_SWAP_ID: SwapId = SwapId(2);

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			// FoK swap 1 should fail and will eventually be refunded
			insert_swaps(&[fok_swap(
				Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					min_output: INPUT_AMOUNT,
				}),
				is_ccm,
			)]);

			// Non-FoK swap 2 will fail together with swap 1, but should be retried indefinitely
			insert_swaps(&[fok_swap(None, is_ccm)]);

			assert_swaps_scheduled_for_block(
				&[FOK_SWAP_ID, REGULAR_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {
			// This simulates not having enough liquidity/triggering price impact protection
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			// Both swaps should fail here and be rescheduled for a later block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: REGULAR_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
					reason: SwapFailureReason::PriceImpactLimit,
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
					reason: SwapFailureReason::PriceImpactLimit,
				}),
			);
		})
		.then_process_blocks_until_block(SWAP_RETRIED_AT_BLOCK)
		.then_execute_with(|_| {
			// Swap request should be removed in case of refund
			assert_eq!(SwapRequests::<Test>::get(FOK_SWAP_REQUEST_ID), None);
			// Swap should fail here (due to price impact protection) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				// Non-fok swap will continue to be retried:
				RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: REGULAR_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(1),
					reason: SwapFailureReason::PriceImpactLimit
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
			);
		});
}

#[test]
fn fok_test_zero_refund_duration_no_ccm() {
	fok_test_zero_refund_duration(false);
}

#[test]
fn fok_test_zero_refund_duration_ccm() {
	fok_test_zero_refund_duration(true);
}

fn fok_test_zero_refund_duration(is_ccm: bool) {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			// A swap with 0 retry duration should be tried exactly 1 time
			insert_swaps(&[fok_swap(
				Some(TestRefundParams { retry_duration: 0, min_output: INPUT_AMOUNT }),
				is_ccm,
			)]);

			assert_swaps_scheduled_for_block(&[1.into()], SWAPS_SCHEDULED_FOR_BLOCK);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {
			// This simulates not having enough liquidity/triggering price impact protection
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			// The swap should fail and be refunded immediately instead of being retried
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::SwapAborted {
					swap_id: SwapId(1),
					reason: SwapFailureReason::PriceImpactLimit
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: SwapRequestId(1),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					..
				}),
			);
		});
}

#[test]
fn test_refund_parameter_validation() {
	use cf_traits::SwapParameterValidation;

	new_test_ext().execute_with(|| {
		let max_swap_retry_duration_blocks = MaxSwapRetryDurationBlocks::<Test>::get();

		assert_ok!(Swapping::validate_refund_params(0));
		assert_ok!(Swapping::validate_refund_params(max_swap_retry_duration_blocks));
		assert_err!(
			Swapping::validate_refund_params(max_swap_retry_duration_blocks + 1),
			DispatchError::from(crate::Error::<Test>::RetryDurationTooHigh)
		);
	});
}

#[test]
fn test_zero_refund_amount_remaining() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.then_execute_at_block(INIT_BLOCK, |_| {
			// Set a refund fee to the swap amount
			NetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::zero(),
				minimum: INPUT_AMOUNT,
			});

			// A swap with 0 retry duration, so it will be refunded immediately
			insert_swaps(&[fok_swap(
				Some(TestRefundParams { retry_duration: 0, min_output: INPUT_AMOUNT }),
				false,
			)]);

			assert_swaps_scheduled_for_block(&[1.into()], SWAPS_SCHEDULED_FOR_BLOCK);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {
			// Trigger a refund
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			// The refund should ignored and all of the swap amount should be swapped for fees
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::SwapAborted { swap_id: SwapId(1), reason: SwapFailureReason::PriceImpactLimit }),
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: SwapRequestId(2),
					input_asset: Asset::Usdc,
					input_amount: INPUT_AMOUNT,
					output_asset: Asset::Flip,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_request_id: SwapRequestId(2),
					input_amount: INPUT_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::RefundEgressIgnored {
					swap_request_id: SwapRequestId(1),
					amount: 0,
					asset: INPUT_ASSET,
					reason,
					..
				}) if reason == DispatchError::from(Error::<Test>::NoRefundAmountRemaining),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(1),
					..
				}),
			);
		});
}

mod oracle_swaps {

	use cf_traits::mocks::price_feed_api::MockPriceFeedApi;

	use super::*;

	#[test]
	fn basic_oracle_swap() {
		const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_RETRY_BLOCK: u64 = CHUNK_2_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64;

		// Must set the retry duration to a non-zero value for the test
		const RETRY_DURATION: u32 = 10;

		const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;

		const SWAP_RATE: u128 = 2;
		const ORACLE_PRICE_SLIPPAGE: BasisPoints = 100; // 1% slippage
		const NEW_SWAP_RATE: f64 = 1.97; // Reduced by more than 1%

		// Set the price to match the swap rate at first
		const OUTPUT_ASSET_PRICE: u128 = 2;
		const INPUT_ASSET_PRICE: u128 = OUTPUT_ASSET_PRICE * SWAP_RATE;

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				MockPriceFeedApi::set_price(
					INPUT_ASSET,
					Some(U256::from(INPUT_ASSET_PRICE) << PRICE_FRACTIONAL_BITS),
				);
				MockPriceFeedApi::set_price(
					OUTPUT_ASSET,
					Some(U256::from(OUTPUT_ASSET_PRICE) << PRICE_FRACTIONAL_BITS),
				);

				// Execution price is exactly the same as the oracle price,
				// so the first chunk should go through
				SwapRate::set(SWAP_RATE as f64);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					RETRY_DURATION,
					PriceLimits {
						// Make sure we turn of the old min price check
						min_price: 0.into(),
						// Set the maximum oracle price slippage to any value
						max_oracle_price_slippage: Some(ORACLE_PRICE_SLIPPAGE),
					},
					Some(DcaParameters { number_of_chunks: 2, chunk_interval: 2 }),
					LP_ACCOUNT,
				);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						input_amount: CHUNK_AMOUNT,
						output_amount,
						..
					}) if *output_amount == CHUNK_AMOUNT * SWAP_RATE
				);

				// Turn the swap rate down to trigger the oracle slippage protection
				SwapRate::set(NEW_SWAP_RATE);
			})
			.then_process_blocks_until_block(CHUNK_2_BLOCK)
			.then_execute_with(|_| {
				// Chunk 2's output was below the price limit, so it will be retried.
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRescheduled {
						reason: SwapFailureReason::OraclePriceSlippageExceeded,
						..
					})
				);

				// Now drop the oracle price for input asset. It will once again match the swap
				// rate, so the swap should succeed.
				MockPriceFeedApi::set_price(
					INPUT_ASSET,
					Some(U256::from(OUTPUT_ASSET_PRICE) << PRICE_FRACTIONAL_BITS),
				);
			})
			.then_process_blocks_until_block(CHUNK_2_RETRY_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						input_amount: CHUNK_AMOUNT,
						output_amount,
						..
					}) if *output_amount == (CHUNK_AMOUNT as f64 * NEW_SWAP_RATE) as u128
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. })
				);
			});
	}

	#[test]
	fn oracle_swap_ignores_oracle_if_not_supported_or_unavailable() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				// Set the price of one of the assets to None to simulate being unsupported
				MockPriceFeedApi::set_price(INPUT_ASSET, None);
				MockPriceFeedApi::set_price(
					OUTPUT_ASSET,
					Some(U256::from(DEFAULT_SWAP_RATE) << PRICE_FRACTIONAL_BITS),
				);

				// Set the swap rate to a small value well below the oracle slippage.
				// So that if the oracle price was used, the swap would fail.
				SwapRate::set(0.000001);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					0, // retry duration
					// Set the oracle price slippage to a non-zero value
					PriceLimits { min_price: 0.into(), max_oracle_price_slippage: Some(10) },
					None,
					LP_ACCOUNT,
				);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { .. })
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. })
				);
			});

		// Also test the output asset being unsupported
		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);
				MockPriceFeedApi::set_price(OUTPUT_ASSET, None);
				MockPriceFeedApi::set_price(
					INPUT_ASSET,
					Some(U256::from(DEFAULT_SWAP_RATE) << PRICE_FRACTIONAL_BITS),
				);

				// Set the swap rate to a small value well below the oracle slippage
				SwapRate::set(0.000001);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					0, // retry duration
					// Set the oracle price slippage to a non-zero value
					PriceLimits { min_price: 0.into(), max_oracle_price_slippage: Some(10) },
					None,
					LP_ACCOUNT,
				);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { .. })
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. })
				);
			});
	}

	#[test]
	fn oracle_swap_aborts_if_price_stale() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const SWAP_RETRIED_AT_BLOCK: u64 = SWAP_BLOCK + (DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64);

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				MockPriceFeedApi::set_price(
					INPUT_ASSET,
					Some(U256::from(2) << PRICE_FRACTIONAL_BITS),
				);
				MockPriceFeedApi::set_price(
					OUTPUT_ASSET,
					Some(U256::from(2) << PRICE_FRACTIONAL_BITS),
				);

				// Set one of the assets to stale
				MockPriceFeedApi::set_stale(INPUT_ASSET, true);
				MockPriceFeedApi::set_stale(OUTPUT_ASSET, false);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					// Set the oracle price slippage to a non-zero value
					PriceLimits { min_price: 0.into(), max_oracle_price_slippage: Some(10) },
					None,
					LP_ACCOUNT,
				);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRescheduled {
						swap_id: SwapId(1),
						execute_at: SWAP_RETRIED_AT_BLOCK,
						reason: SwapFailureReason::OraclePriceStale,
					})
				);

				// Change to the other asset being stale
				MockPriceFeedApi::set_stale(INPUT_ASSET, false);
				MockPriceFeedApi::set_stale(OUTPUT_ASSET, true);
			})
			.then_process_blocks_until_block(SWAP_RETRIED_AT_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapAborted {
						swap_id: SwapId(1),
						reason: SwapFailureReason::OraclePriceStale
					})
				);

				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapRequestCompleted { .. })
				);
			});
	}
}
