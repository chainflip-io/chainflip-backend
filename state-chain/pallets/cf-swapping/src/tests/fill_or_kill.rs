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

			let refund_parameters_encoded = REFUND_PARAMS
				.into_extended_params(INPUT_AMOUNT)
				.to_encoded::<MockAddressConverter>();

			insert_swaps(&vec![fok_swap(None, is_ccm), fok_swap(Some(REFUND_PARAMS), is_ccm)]);

			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequested {
					swap_request_id: FOK_REQUEST_ID,
					refund_parameters,
					..
				}) if refund_parameters.as_ref() == Some(&refund_parameters_encoded),
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
	const HIGH_OUTPUT: AssetAmount = EXPECTED_OUTPUT + 1;

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
					execute_at: SWAP_RETRIED_AT_BLOCK
				}),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 1);
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

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 0);
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
			const MIN_OUTPUT: AssetAmount = (INPUT_AMOUNT - BROKER_FEE) * DEFAULT_SWAP_RATE + 1;
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
			NetworkFee::set(Permill::from_percent(1));

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
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
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
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
				// Non-fok swap will continue to be retried:
				RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: REGULAR_SWAP_ID, .. }),
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
	use cf_traits::SwapLimitsProvider;

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
