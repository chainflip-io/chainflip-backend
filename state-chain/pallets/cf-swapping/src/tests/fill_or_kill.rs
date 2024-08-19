use super::*;

const INPUT_AMOUNT: AssetAmount = 40_000;

fn new_swap(refund_params: Option<TestRefundParams>) -> TestSwapParams {
	TestSwapParams {
		input_asset: Asset::Eth,
		output_asset: Asset::Usdc,
		input_amount: INPUT_AMOUNT,
		refund_params: refund_params.map(|params| params.into_channel_params(INPUT_AMOUNT)),
		dca_params: None,
		output_address: ForeignChainAddress::Eth([1; 20].into()),
		is_ccm: false,
	}
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
fn both_fok_and_regular_swaps_succeed_first_try() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	const REGULAR_SWAP_ID: u64 = 1;
	const FOK_SWAP_ID: u64 = 2;

	const REGULAR_REQUEST_ID: u64 = 1;
	const FOK_REQUEST_ID: u64 = 2;

	const BROKER_FEE: AssetAmount =
		INPUT_AMOUNT * DEFAULT_SWAP_RATE * BROKER_FEE_BPS as u128 / 10_000;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&vec![
				new_swap(None),
				new_swap(Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u32,
					min_output: INPUT_AMOUNT * DEFAULT_SWAP_RATE - BROKER_FEE,
				})),
			]);

			assert_swaps_scheduled_for_block(
				&[REGULAR_SWAP_ID, FOK_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {})
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
fn price_limit_is_respected_in_fok_swap() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 = SWAPS_SCHEDULED_FOR_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;

	const BROKER_FEE: AssetAmount =
		INPUT_AMOUNT * DEFAULT_SWAP_RATE * BROKER_FEE_BPS as u128 / 10_000;

	const EXPECTED_OUTPUT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE - BROKER_FEE;
	const HIGH_OUTPUT: AssetAmount = EXPECTED_OUTPUT + 1;

	const REGULAR_SWAP_ID: u64 = 1;
	const FOK_SWAP_1_ID: u64 = 2;
	const FOK_SWAP_2_ID: u64 = 3;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&vec![
				new_swap(None),
				new_swap(Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u32,
					min_output: HIGH_OUTPUT,
				})),
				new_swap(Some(TestRefundParams {
					retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u32,
					min_output: EXPECTED_OUTPUT,
				})),
			]);

			assert_swaps_scheduled_for_block(
				&[REGULAR_SWAP_ID, FOK_SWAP_1_ID, FOK_SWAP_2_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_execute_at_block(3u64, |_| {})
		.then_execute_with(|_| {
			assert_eq!(System::block_number(), SWAPS_SCHEDULED_FOR_BLOCK);
			// Swap 2 should fail due to price limit and rescheduled for block
			// `SWAPS_SCHEDULED_FOR_BLOCK + SWAP_RETRY_DELAY_BLOCKS`, but swaps 1 and 3 should be
			// successful:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_1_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: REGULAR_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1 }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_2_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 3 }),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 1);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {
			// Changing the swap rate to allow the FoK swap to be executed
			SwapRate::set((HIGH_OUTPUT + BROKER_FEE) as f64 / INPUT_AMOUNT as f64);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: FOK_SWAP_1_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_request_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 2 }),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 0);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_limit() {
	const FOK_SWAP_REQUEST_ID: u64 = 1;
	const OTHER_SWAP_REQUEST_ID: u64 = 2;

	const FOK_SWAP_ID: u64 = 1;
	const OTHER_SWAP_ID: u64 = 2;

	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 = SWAPS_SCHEDULED_FOR_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			// Min output for swap 1 is too high to be executed:
			const MIN_OUTPUT: AssetAmount = INPUT_AMOUNT * DEFAULT_SWAP_RATE + 1;
			insert_swaps(&[new_swap(Some(TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u32,
				min_output: MIN_OUTPUT,
			}))]);
			// However, swap 2 is non-FoK and should still be executed:
			insert_swaps(&[new_swap(None)]);

			assert_swaps_scheduled_for_block(
				&[FOK_SWAP_ID, OTHER_SWAP_ID],
				SWAPS_SCHEDULED_FOR_BLOCK,
			);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap 1 should fail here and rescheduled for a later block,
			// but swap 2 (without FoK parameters) should still be successful:
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				}),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: OTHER_SWAP_ID, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: OTHER_SWAP_REQUEST_ID,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: OTHER_SWAP_REQUEST_ID
				}),
			);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap request should be removed in case of refund
			assert_eq!(SwapRequests::<Test>::get(FOK_SWAP_REQUEST_ID), None);
			// Swap should fail here (due to price limit) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
			);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_impact_protection() {
	const FOK_SWAP_REQUEST_ID: u64 = 1;

	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;
	const SWAP_RETRIED_AT_BLOCK: u64 = SWAPS_SCHEDULED_FOR_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;

	const FOK_SWAP_ID: u64 = 1;
	const REGULAR_SWAP_ID: u64 = 2;

	new_test_ext()
		.execute_with(|| {
			// FoK swap 1 should fail and will eventually be refunded
			insert_swaps(&[new_swap(Some(TestRefundParams {
				retry_duration: DEFAULT_SWAP_RETRY_DELAY_BLOCKS as u32,
				min_output: INPUT_AMOUNT,
			}))]);

			// Non-FoK swap 2 will fail together with swap 1, but should be retried indefinitely
			insert_swaps(&[new_swap(None)]);

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
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: FOK_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				}),
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: REGULAR_SWAP_ID,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				})
			);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap request should be removed in case of refund
			assert_eq!(SwapRequests::<Test>::get(FOK_SWAP_REQUEST_ID), None);
			// Swap should fail here (due to price impact protection) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: FOK_SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled {
					swap_request_id: FOK_SWAP_REQUEST_ID,
					..
				}),
				// Non-fok swap will continue to be retried:
				RuntimeEvent::Swapping(Event::SwapRescheduled { swap_id: REGULAR_SWAP_ID, .. }),
			);
		});
}

#[test]
fn fok_test_zero_refund_duration() {
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = INIT_BLOCK + SWAP_DELAY_BLOCKS as u64;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			assert_ok!(Swapping::init_swap_request(
				Asset::Eth,
				INPUT_AMOUNT,
				Asset::Usdc,
				SwapRequestType::Regular {
					output_address: ForeignChainAddress::Eth([1; 20].into())
				},
				bounded_vec![Beneficiary { account: 0u64, bps: 2 }],
				Some(ChannelRefundParameters {
					// Set the retry duration to 0 blocks
					retry_duration: 0,
					refund_address: ForeignChainAddress::Eth([10; 20].into()),
					min_price: U256::zero(),
				}),
				None,
				SwapOrigin::Vault { tx_hash: Default::default() },
			));

			assert_swaps_scheduled_for_block(&[1], SWAPS_SCHEDULED_FOR_BLOCK);
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
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1, .. }),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled { swap_request_id: 1, .. }),
			);
		});
}
