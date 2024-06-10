use super::*;

const INPUT_AMOUNT: AssetAmount = 40;

fn new_swap(id: SwapId, refund_params: Option<SwapRefundParameters>) -> Swap {
	let swap_type = SwapType::Swap(ForeignChainAddress::Eth([9; 20].into()));
	Swap::new(id, Asset::Eth, Asset::Usdc, INPUT_AMOUNT, refund_params, swap_type)
}

fn params(refund_if_reaches: u32, min_output: AssetAmount) -> SwapRefundParameters {
	SwapRefundParameters {
		refund_block: refund_if_reaches,
		refund_address: ForeignChainAddress::Eth([10; 20].into()),
		min_output,
	}
}

#[test]
fn price_limit_is_respected_in_fok_swap() {
	const SWAPS_ADDED_BLOCK: u64 = 1;
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = 3;
	const SWAP_RETRIED_AT_BLOCK: u64 = SWAPS_SCHEDULED_FOR_BLOCK + SWAP_RETRY_DELAY_BLOCKS as u64;

	const HIGH_MIN_OUTPUT: AssetAmount = INPUT_AMOUNT * 2;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), SWAPS_ADDED_BLOCK);

			insert_swaps(&vec![
				new_swap(1, None),
				new_swap(2, Some(params(5, HIGH_MIN_OUTPUT))),
				new_swap(3, Some(params(5, INPUT_AMOUNT))),
			]);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 1,
					execute_at: SWAPS_SCHEDULED_FOR_BLOCK,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 2,
					execute_at: SWAPS_SCHEDULED_FOR_BLOCK,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 3,
					execute_at: SWAPS_SCHEDULED_FOR_BLOCK,
					..
				}),
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
					swap_id: 2,
					execute_at: SWAP_RETRIED_AT_BLOCK
				}),
				RuntimeEvent::Swapping(Event::NetworkFeeTaken { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::NetworkFeeTaken { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 1, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 3, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 3, .. }),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 1);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {
			// Changing the swap rate to allow the FoK swap to be executed
			SwapRate::set(HIGH_MIN_OUTPUT as f64 / INPUT_AMOUNT as f64);
		})
		.then_execute_with(|_| {
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::NetworkFeeTaken { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapExecuted { swap_id: 2, .. }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled { swap_id: 2, .. }),
			);

			assert_eq!(SwapQueue::<Test>::get(SWAP_RETRIED_AT_BLOCK).len(), 0);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_price_limit() {
	const SWAPS_ADDED_BLOCK: u64 = 1;
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = 3;
	const SWAP_RETRIED_AT_BLOCK: u64 = SWAPS_SCHEDULED_FOR_BLOCK + SWAP_RETRY_DELAY_BLOCKS as u64;
	// The swap will be refunded after the first retry:
	const SWAP_REFUND_AT_BLOCK: u32 = SWAP_RETRIED_AT_BLOCK as u32;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), SWAPS_ADDED_BLOCK);

			// Output is too high to be executed:
			const MIN_OUTPUT: AssetAmount = INPUT_AMOUNT * 2;

			insert_swaps(&[new_swap(1, Some(params(SWAP_REFUND_AT_BLOCK, MIN_OUTPUT)))]);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 1,
					execute_at: SWAPS_SCHEDULED_FOR_BLOCK,
					..
				}),
			);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap should fail here and resheduled for a later block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 1,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				})
			);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap should fail here (due to price limit) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::RefundEgressScheduled { swap_id: 1, .. })
			);
		});
}

#[test]
fn fok_swap_gets_refunded_due_to_slippage_protection() {
	const SWAPS_ADDED_BLOCK: u64 = 1;
	const SWAPS_SCHEDULED_FOR_BLOCK: u64 = 3;
	const SWAP_RETRIED_AT_BLOCK: u64 = SWAPS_SCHEDULED_FOR_BLOCK + SWAP_RETRY_DELAY_BLOCKS as u64;
	// The swap will be refunded after the first retry:
	const SWAP_REFUND_AT_BLOCK: u32 = SWAP_RETRIED_AT_BLOCK as u32;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), SWAPS_ADDED_BLOCK);

			// Output is too high to be executed:
			const MIN_OUTPUT: AssetAmount = INPUT_AMOUNT * 2;

			insert_swaps(&[new_swap(1, Some(params(SWAP_REFUND_AT_BLOCK, MIN_OUTPUT)))]);

			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapScheduled {
					swap_id: 1,
					execute_at: SWAPS_SCHEDULED_FOR_BLOCK,
					..
				}),
			);
		})
		.then_execute_at_block(SWAPS_SCHEDULED_FOR_BLOCK, |_| {})
		.then_execute_with(|_| {
			// Swap should fail here and be rescheduled for a later block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::SwapRescheduled {
					swap_id: 1,
					execute_at: SWAP_RETRIED_AT_BLOCK,
				})
			);
		})
		.then_execute_at_block(SWAP_RETRIED_AT_BLOCK, |_| {
			// This simulates not having enough liquidity/triggering slippage protection
			MockSwappingApi::set_swaps_should_fail(true);
		})
		.then_execute_with(|_| {
			// Swap should fail here (due to slippage protection) and be refunded due
			// to reaching expiry block
			assert_event_sequence!(
				Test,
				RuntimeEvent::Swapping(Event::BatchSwapFailed { .. }),
				RuntimeEvent::Swapping(Event::RefundEgressScheduled { swap_id: 1, .. }),
			);
		});
}
