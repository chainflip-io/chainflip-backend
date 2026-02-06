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

use cf_traits::mocks::price_feed_api::MockPriceFeedApi;
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

			insert_swaps(&[fok_swap(None, is_ccm), fok_swap(Some(REFUND_PARAMS), is_ccm)]);

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
					swap_request_id: REGULAR_REQUEST_ID,
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: FOK_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_REQUEST_ID,
					reason: SwapRequestCompletionReason::Executed
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
			insert_swaps(&[
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
					swap_request_id: SwapRequestId(1),
					reason: SwapRequestCompletionReason::Executed
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_2_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SwapRequestId(3),
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SwapRequestId(3),
					reason: SwapRequestCompletionReason::Executed
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
					swap_request_id: SwapRequestId(2),
					reason: SwapRequestCompletionReason::Executed
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
					swap_request_id: OTHER_SWAP_REQUEST_ID,
					reason: SwapRequestCompletionReason::Executed
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
					swap_request_id: FOK_SWAP_REQUEST_ID,
					reason: SwapRequestCompletionReason::Expired
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
					swap_request_id: FOK_SWAP_REQUEST_ID,
					reason: SwapRequestCompletionReason::Expired
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
	new_test_ext().execute_with(|| {
		let max_swap_retry_duration_blocks = MaxSwapRetryDurationBlocks::<Test>::get();

		assert_ok!(Swapping::validate_refund_params(INPUT_ASSET, OUTPUT_ASSET, 0, None));
		assert_ok!(Swapping::validate_refund_params(
			INPUT_ASSET,
			OUTPUT_ASSET,
			max_swap_retry_duration_blocks,
			None
		));
		assert_err!(
			Swapping::validate_refund_params(
				INPUT_ASSET,
				OUTPUT_ASSET,
				max_swap_retry_duration_blocks + 1,
				None
			),
			DispatchError::from(crate::Error::<Test>::RetryDurationTooHigh)
		);

		MockPriceFeedApi::set_price(INPUT_ASSET, None);
		MockPriceFeedApi::set_price(OUTPUT_ASSET, Some(Price::from_raw(U256::one())));
		assert_err!(
			Swapping::validate_refund_params(INPUT_ASSET, OUTPUT_ASSET, 0, Some(100)),
			DispatchError::from(crate::Error::<Test>::OraclePriceNotAvailable)
		);
		MockPriceFeedApi::set_price(INPUT_ASSET, Some(Price::from_raw(U256::one())));
		MockPriceFeedApi::set_price(OUTPUT_ASSET, None);
		assert_err!(
			Swapping::validate_refund_params(INPUT_ASSET, OUTPUT_ASSET, 0, Some(100)),
			DispatchError::from(crate::Error::<Test>::OraclePriceNotAvailable)
		);
		MockPriceFeedApi::set_price(OUTPUT_ASSET, Some(Price::from_raw(U256::one())));
		assert_ok!(Swapping::validate_refund_params(INPUT_ASSET, OUTPUT_ASSET, 0, Some(100)));
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
	use super::*;
	use cf_traits::mocks::price_feed_api::MockPriceFeedApi;

	#[test]
	fn basic_oracle_swap() {
		// We want to test a 2 leg swap with oracle price slippage protection.
		const INPUT_ASSET: Asset = Asset::Eth;
		const OUTPUT_ASSET: Asset = Asset::Btc;

		const CHUNK_1_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + SWAP_DELAY_BLOCKS as u64;
		const CHUNK_2_RETRY_BLOCK: u64 = CHUNK_2_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u64;

		// Must set the retry duration to a non-zero value for the test
		const RETRY_DURATION: u32 = 10;

		const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;

		const SWAP_RATE: u128 = 2;
		const ORACLE_PRICE_SLIPPAGE: BasisPoints = 100; // 1% slippage
		const NEW_SWAP_RATE: f64 = 1.97; // Reduced by more than 1%

		// Set the price to match the swap rate for each leg
		const OUTPUT_ASSET_PRICE: u128 = 100_000_000;
		const STABLE_PRICE: u128 = OUTPUT_ASSET_PRICE * SWAP_RATE;
		const INPUT_ASSET_PRICE: u128 = STABLE_PRICE * SWAP_RATE;

		const NETWORK_FEE_BPS: u32 = 100;
		const BROKER_FEE_BPS: u16 = 100;
		let network_fee = Permill::from_parts(NETWORK_FEE_BPS * 100);
		// Using a large enough minimum that it will be applied in the test to ensure the oracle
		// price protection does not trigger on the first chunk because of it.
		let network_fee_minimum = network_fee * CHUNK_AMOUNT * 2;

		// Also checking the oracle delta value is set correctly (with rounding error)
		let expected_oracle_delta =
			Some(SignedBasisPoints::negative_slippage(NETWORK_FEE_BPS as u16 + BROKER_FEE_BPS));

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, INPUT_ASSET_PRICE);
				MockPriceFeedApi::set_price_usd_fine(OUTPUT_ASSET, OUTPUT_ASSET_PRICE);
				MockPriceFeedApi::set_price_usd_fine(STABLE_ASSET, STABLE_PRICE);

				NetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: network_fee,
					minimum: network_fee_minimum,
				});

				// Execution price is exactly the same as the oracle price,
				// so the first chunk should go through
				SwapRate::set(SWAP_RATE as f64);

				Swapping::init_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::Egress {
							output_address: ForeignChainAddress::Eth([2; 20].into()),
							ccm_deposit_metadata: None,
						},
					},
					vec![Beneficiary { account: BROKER, bps: BROKER_FEE_BPS }].try_into().unwrap(),
					Some(PriceLimitsAndExpiry {
						expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
							retry_duration: RETRY_DURATION,
							refund_address: AccountOrAddress::InternalAccount(LP_ACCOUNT),
							refund_ccm_metadata: None,
						},
						// Make sure we turn off the old min price check
						min_price: Price::zero(),
						// Set the maximum oracle price slippage to any value
						max_oracle_price_slippage: Some(ORACLE_PRICE_SLIPPAGE),
					}),
					Some(DcaParameters { number_of_chunks: 2, chunk_interval: 2 }),
					SwapOrigin::OnChainAccount(LP_ACCOUNT),
				);
			})
			.then_process_blocks_until_block(CHUNK_1_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(
						Event::SwapExecuted {
						input_amount: CHUNK_AMOUNT,
						network_fee,
						oracle_delta,
						..
					},
					// Make sure the network fee minimum was taken
					) if *network_fee == network_fee_minimum && *oracle_delta == expected_oracle_delta
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
				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, OUTPUT_ASSET_PRICE);
			})
			.then_process_blocks_until_block(CHUNK_2_RETRY_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted { input_amount: CHUNK_AMOUNT, .. })
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
				MockPriceFeedApi::set_price_usd_fine(OUTPUT_ASSET, DEFAULT_SWAP_RATE);

				// Set the swap rate to a small value well below the oracle slippage.
				// So that if the oracle price was used, the swap would fail.
				SwapRate::set(0.000001);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					0, // retry duration
					// Set the oracle price slippage to a non-zero value
					PriceLimits { min_price: Price::zero(), max_oracle_price_slippage: Some(10) },
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
				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, DEFAULT_SWAP_RATE);

				// Set the swap rate to a small value well below the oracle slippage
				SwapRate::set(0.000001);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					0, // retry duration
					// Set the oracle price slippage to a non-zero value
					PriceLimits { min_price: Price::zero(), max_oracle_price_slippage: Some(10) },
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

				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, 2);
				MockPriceFeedApi::set_price_usd_fine(OUTPUT_ASSET, 2);

				// Set one of the assets to stale
				MockPriceFeedApi::set_stale(INPUT_ASSET, true);
				MockPriceFeedApi::set_stale(OUTPUT_ASSET, false);

				Swapping::init_internal_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					DEFAULT_SWAP_RETRY_DELAY_BLOCKS,
					// Set the oracle price slippage to a non-zero value
					PriceLimits { min_price: Price::zero(), max_oracle_price_slippage: Some(10) },
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

	#[test]
	fn test_negative_oracle_price_delta() {
		// The swap output will be lower than the oracle
		const SWAP_RATE_BPS: u32 = 100;
		const NETWORK_FEE_BPS: u32 = 100;
		const BROKER_FEE_BPS: u16 = 100;

		// The expected delta is 0.99^3-1 = -0.029701 = -297.01 bps, rounded away from zero to -298
		const EXPECTED_DELTA: Option<SignedBasisPoints> = Some(SignedBasisPoints(-298));
		const EXPECTED_DELTA_EX_FEES: Option<SignedBasisPoints> =
			Some(SignedBasisPoints(-(SWAP_RATE_BPS as i32 + 1))); // Small rounding error

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				// Set the fees, price and swap rate so we get the exact delta we want
				NetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_parts(NETWORK_FEE_BPS * 100),
					minimum: 0,
				});
				SwapRate::set(1.0 - (SWAP_RATE_BPS as f64 / 10000.0));
				// We use a price of 1 for all assets to make the math easier
				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, 1);
				MockPriceFeedApi::set_price_usd_fine(OUTPUT_ASSET, 1);
				MockPriceFeedApi::set_price_usd_fine(STABLE_ASSET, 1);

				Swapping::init_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::Egress {
							output_address: ForeignChainAddress::Eth([2; 20].into()),
							ccm_deposit_metadata: None,
						},
					},
					vec![Beneficiary { account: BROKER, bps: BROKER_FEE_BPS }].try_into().unwrap(),
					// No oracle price slippage protection, but we should still get both oracle
					// delta's in the event
					None,
					None,
					SwapOrigin::OnChainAccount(0_u64),
				);
			})
			.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						oracle_delta: EXPECTED_DELTA,
						oracle_delta_ex_fees: EXPECTED_DELTA_EX_FEES,
						..
					})
				);
			});
	}

	#[test]
	fn can_handle_positive_oracle_price_delta() {
		// The swap output will be higher than the oracle
		const SWAP_RATE_BPS: u32 = 100;
		const NETWORK_FEE_BPS: u32 = 10;
		const BROKER_FEE_BPS: u16 = 10;
		const EXPECTED_DELTA: Option<SignedBasisPoints> = Some(SignedBasisPoints(80));

		new_test_ext()
			.execute_with(|| {
				assert_eq!(System::block_number(), INIT_BLOCK);

				// Set the fees, price and swap rate so we get the exact delta we want
				NetworkFee::<Test>::set(FeeRateAndMinimum {
					rate: Permill::from_parts(NETWORK_FEE_BPS * 100),
					minimum: 0,
				});
				// Using a positive swap rate
				SwapRate::set(1.0 + (SWAP_RATE_BPS as f64 / 10000.0));
				// We use a price of 1 for both assets to make the math easier
				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, 1);
				MockPriceFeedApi::set_price_usd_fine(OUTPUT_ASSET, 1);

				Swapping::init_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::Egress {
							output_address: ForeignChainAddress::Eth([2; 20].into()),
							ccm_deposit_metadata: None,
						},
					},
					vec![Beneficiary { account: BROKER, bps: BROKER_FEE_BPS }].try_into().unwrap(),
					None,
					None,
					SwapOrigin::OnChainAccount(0_u64),
				);
			})
			.then_process_blocks_until_block(INIT_BLOCK + SWAP_DELAY_BLOCKS as u64)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapExecuted {
						oracle_delta: EXPECTED_DELTA,
						..
					})
				);
			});
	}

	mod oracle_swap_calculations_with_real_world_values {
		use cf_primitives::basis_points::SignedHundredthBasisPoints;

		use super::*;

		// Values from an actual swap
		const INPUT_AMOUNT: AssetAmount = 9000632;
		const OUTPUT_AMOUNT: AssetAmount = 2695410274420764757;
		const STABLE_AMOUNT: AssetAmount = 8020476946;
		const BROKER_FEE: AssetAmount = 12048789; // 15 bps
		const NETWORK_FEE: AssetAmount = 8040566; // 10 bps

		fn set_prices() {
			// Prices taken at similar time to the swap values above
			let eth_price = Price::from_usd(Asset::Eth, 2972);
			let btc_price = Price::from_usd(Asset::Btc, 89487);
			let usdc_price = Price::from_raw(
				U256::from_dec_str("340214310447554275770681932510281857813").unwrap(), // $0.9997
			);

			MockPriceFeedApi::set_price(Asset::Eth, Some(eth_price));
			MockPriceFeedApi::set_price(Asset::Btc, Some(btc_price));
			MockPriceFeedApi::set_price(Asset::Usdc, Some(usdc_price));
		}

		fn test_swap_state(max_oracle_price_slippage: Option<BasisPoints>) -> SwapState<Test> {
			SwapState {
				swap: Swap::new(
					0.into(),
					0.into(),
					Asset::Btc,
					Asset::Eth,
					INPUT_AMOUNT,
					Some(SwapRefundParameters {
						refund_block: 10,
						price_limits: PriceLimits {
							min_price: Price::zero(),
							max_oracle_price_slippage,
						},
					}),
					Default::default(),
				),

				network_fee_taken: Some(NETWORK_FEE),
				broker_fee_taken: Some(BROKER_FEE),
				stable_amount: Some(STABLE_AMOUNT),
				final_output: Some(OUTPUT_AMOUNT),
				oracle_delta: None,
				oracle_delta_ex_fees: None,
			}
		}

		#[test]
		fn oracle_delta_real_world_values() {
			// Relative price = 89487 / 2972 = 30.110026917900402 Eth per Btc
			// Oracle output amount = 0.09000632 * 30.110026917900402 = 2.710092717981157 Eth
			// Total delta = (( 2.695410274420764757 / 2.710092717981157 ) - 1) * 10000 = -54.18 bps
			// Sanity check by adding fees to slippage = 29 + 15 + 10 = 54
			const EXPECTED_DELTA_BPS: SignedHundredthBasisPoints =
				SignedHundredthBasisPoints(-5418);

			new_test_ext().execute_with(|| {
				set_prices();
				let swap_state = test_swap_state(None);
				let oracle_delta = Pallet::<Test>::get_delta_from_oracle_price(
					swap_state.input_amount(),
					swap_state.final_output.unwrap_or(0),
					swap_state.input_asset(),
					swap_state.output_asset(),
				)
				.unwrap()
				.unwrap();
				assert_eq!(oracle_delta, EXPECTED_DELTA_BPS);
			});
		}

		#[test]
		fn oracle_swap_price_violation_real_world_values() {
			// Stable amount before fees = 8020476946 + 12048789 + 8040566 = $8040.566301
			// Oracle stable amount = 0.09000632 * 89487 = $8054.4
			// delta on first leg = ((8040.6 / 8054.4) - 1) * 10000 = -17.13 bps
			// Eth oracle amount = 8020.476946 / 2972 = 2.698679995 Eth
			// Delta on second leg = (( 2.695410274 / 2.698679995 ) - 1) * 10000 = -12.1 bps
			// Total slippage = 12.1 + 17.13 = 29.23 bps
			// => So a slippage limit of 30 bps should pass, while 29 bps should fail
			const EXPECTED_FAILING_SLIPPAGE_LIMIT: BasisPoints = 29;
			const EXPECTED_ORACLE_DELTA_EX_FEES: SignedBasisPoints = SignedBasisPoints(-30);

			new_test_ext().execute_with(|| {
				set_prices();

				// Oracle slippage that is below or equal to the slippage limit will pass
				assert_eq!(
					Pallet::<Test>::check_swap_price_violation(&test_swap_state(Some(
						EXPECTED_FAILING_SLIPPAGE_LIMIT + 1
					))),
					Ok(Some(EXPECTED_ORACLE_DELTA_EX_FEES))
				);

				// Oracle delta that is above the slippage limit will fail
				assert_err!(
					Pallet::<Test>::check_swap_price_violation(&test_swap_state(Some(
						EXPECTED_FAILING_SLIPPAGE_LIMIT
					))),
					SwapFailureReason::OraclePriceSlippageExceeded
				);
			});
		}
	}

	#[test]
	fn will_use_default_oracle_price_protection() {
		const INPUT_ASSET: Asset = Asset::Eth;
		const OUTPUT_ASSET: Asset = Asset::Btc;
		const INPUT_PROTECTION_BPS: BasisPoints = 100;
		const OUTPUT_PROTECTION_BPS: BasisPoints = 200;
		// We want a non-zero network fee and broker fee to ensure they don't affect the
		// price protection calculation.
		const NETWORK_FEE_BPS: BasisPoints = 50;
		const BROKER1_FEE_BPS: BasisPoints = 15;

		const EXPECTED_PRICE_PROTECTION_BPS: BasisPoints =
			INPUT_PROTECTION_BPS + OUTPUT_PROTECTION_BPS;

		new_test_ext().execute_with(|| {
			// Set the price, default oracle protections and network fee.
			MockPriceFeedApi::set_price_usd(INPUT_ASSET, 10_000_000);
			MockPriceFeedApi::set_price_usd(OUTPUT_ASSET, 40_000_000);
			DefaultOraclePriceSlippageProtection::<Test>::set(
				AssetPair::new(INPUT_ASSET, STABLE_ASSET).unwrap(),
				Some(INPUT_PROTECTION_BPS),
			);
			DefaultOraclePriceSlippageProtection::<Test>::set(
				AssetPair::new(OUTPUT_ASSET, STABLE_ASSET).unwrap(),
				Some(OUTPUT_PROTECTION_BPS),
			);
			InternalSwapNetworkFee::<Test>::set(FeeRateAndMinimum {
				rate: Permill::from_parts(NETWORK_FEE_BPS as u32 * 100),
				minimum: 0,
			});

			// Init a swap request that has no oracle price protection set. Triggering the
			// default to be calculated and used.
			let _ = Swapping::init_swap_request(
				INPUT_ASSET,
				INPUT_AMOUNT,
				OUTPUT_ASSET,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::CreditOnChain { account_id: 1 },
				},
				vec![Beneficiary { account: BROKER, bps: BROKER1_FEE_BPS }].try_into().unwrap(),
				Some(PriceLimitsAndExpiry {
					expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
						retry_duration: SWAP_DELAY_BLOCKS,
						refund_address: AccountOrAddress::InternalAccount(1),
						refund_ccm_metadata: None,
					},
					min_price: Price::zero(),
					// No max oracle slippage is set
					max_oracle_price_slippage: None,
				}),
				None,
				SwapOrigin::OnChainAccount(0),
			);

			// Check the event for the adjusted price protection
			assert_has_matching_event!(
			Test,
			RuntimeEvent::Swapping(Event::SwapRequested {
				price_limits_and_expiry,
				..
			}) if *price_limits_and_expiry == Some(PriceLimitsAndExpiry {
				expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
					retry_duration: SWAP_DELAY_BLOCKS,
					refund_address: AccountOrAddress::InternalAccount(1),
					refund_ccm_metadata: None,
				},
				min_price: Price::zero(),
				// Just the max oracle slippage has been changed
				max_oracle_price_slippage: Some(EXPECTED_PRICE_PROTECTION_BPS),
			}));
		});
	}

	/// A single-sided oracle swap is where one of the assets supports oracle price but the other
	/// does not. In this case, we should still be able to use oracle price protection for the
	/// supported asset.
	#[test]
	fn single_sided_oracle_swap() {
		const SWAP_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64; // TODO JAMIE: can we factor this out? .then_process_blocks(

		const INPUT_ASSET: Asset = Asset::Eth;
		const OUTPUT_ASSET: Asset = Asset::Flip;
		const INPUT_PROTECTION_BPS: BasisPoints = 100;

		new_test_ext()
			.execute_with(|| {
				// Set the price and default oracle protection for just one asset
				MockPriceFeedApi::set_price_usd_fine(INPUT_ASSET, 10_000_000);
				DefaultOraclePriceSlippageProtection::<Test>::set(
					AssetPair::new(INPUT_ASSET, STABLE_ASSET).unwrap(),
					Some(INPUT_PROTECTION_BPS),
				);
				// USDC needs to also have a price set for the oracle price protection to work.
				MockPriceFeedApi::set_price_usd_fine(STABLE_ASSET, 10_000_000 * DEFAULT_SWAP_RATE);

				// Init a swap request that has no oracle price protection set. Triggering the
				// default to be calculated and used.
				let _ = Swapping::init_swap_request(
					INPUT_ASSET,
					INPUT_AMOUNT,
					OUTPUT_ASSET,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditOnChain { account_id: 1 },
					},
					vec![].try_into().unwrap(),
					Some(PriceLimitsAndExpiry {
						expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
							retry_duration: 0,
							refund_address: AccountOrAddress::InternalAccount(1),
							refund_ccm_metadata: None,
						},
						min_price: Price::zero(),
						// No max oracle slippage is set
						max_oracle_price_slippage: None,
					}),
					None,
					SwapOrigin::OnChainAccount(0),
				);

				// Check the event for the adjusted price protection
				assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					price_limits_and_expiry,
					..
				}) if *price_limits_and_expiry == Some(PriceLimitsAndExpiry {
					expiry_behaviour: ExpiryBehaviour::RefundIfExpires {
						retry_duration: 0,
						refund_address: AccountOrAddress::InternalAccount(1),
						refund_ccm_metadata: None,
					},
					min_price: Price::zero(),
					// Just the max oracle slippage has been changed
					max_oracle_price_slippage: Some(INPUT_PROTECTION_BPS),
				}));

				// Set the swap rate so the swap will fail
				SwapRate::set(0.1);
			})
			.then_process_blocks_until_block(SWAP_BLOCK)
			.then_execute_with(|_| {
				assert_has_matching_event!(
					Test,
					RuntimeEvent::Swapping(Event::SwapAborted {
						reason: SwapFailureReason::OraclePriceSlippageExceeded,
						..
					})
				);
			});
	}
}
