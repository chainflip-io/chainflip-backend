use super::*;

const INPUT_AMOUNT: AssetAmount = 40_000;
const CHUNK_INTERVAL: u32 = 2;
const BROKER_FEE: AssetAmount = INPUT_AMOUNT * BROKER_FEE_BPS as u128 / 10_000;
const NET_AMOUNT: AssetAmount = INPUT_AMOUNT - BROKER_FEE;

const INPUT_ASSET: Asset = Asset::Usdc;
const OUTPUT_ASSET: Asset = Asset::Eth;

fn params(
	dca_params: Option<DcaParameters>,
	refund_params: Option<SwapRefundParameters>,
	is_ccm: bool,
) -> TestSwapParams {
	TestSwapParams {
		input_asset: INPUT_ASSET,
		output_asset: OUTPUT_ASSET,
		input_amount: INPUT_AMOUNT,
		refund_params,
		dca_params,
		output_address: (*EVM_OUTPUT_ADDRESS).clone(),
		is_ccm,
	}
}

#[test]
fn dca_happy_path() {
	const CHUNK_1_BLOCK: u64 = 3;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / 2;

	// 1:1 swap ratio
	const TOTAL_OUTPUT_AMOUNT: AssetAmount = NET_AMOUNT;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters { number_of_chunks: 2, chunk_interval: CHUNK_INTERVAL }),
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
					input_amount: CHUNK_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
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
					output_amount: CHUNK_AMOUNT,
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
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					amount: TOTAL_OUTPUT_AMOUNT,
					..
				})
			);
		});
}

/// Test that DCA with 1 chunk behaves like a regular swap
#[test]
fn dca_single_chunk() {
	const CHUNK_1_BLOCK: u64 = 3;

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
					input_amount: NET_AMOUNT,
					execute_at: CHUNK_1_BLOCK,
					..
				})
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
					input_amount: NET_AMOUNT,
					output_amount: NET_AMOUNT,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					amount: NET_AMOUNT,
					..
				})
			);
		});
}

#[test]
fn dca_with_fok_full_refund() {
	const CHUNK_1_BLOCK: u64 = 3;
	const NUMBER_OF_CHUNKS: u32 = 2;
	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / NUMBER_OF_CHUNKS as u128;

	// Allow for one retry for good measure:
	const REFUND_BLOCK: u64 = CHUNK_1_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: CHUNK_INTERVAL,
				}),
				Some(SwapRefundParameters {
					refund_block: REFUND_BLOCK as u32,
					// Due to 1:1 swap rate, this ensures the swap is refunded:
					min_output: INPUT_AMOUNT + 1,
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
	const CHUNK_1_BLOCK: u64 = 3;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;
	const NUMBER_OF_CHUNKS: u32 = 4;
	const CHUNK_AMOUNT: AssetAmount = NET_AMOUNT / NUMBER_OF_CHUNKS as u128;

	// The test will be set up as to execute one chunk only and refund the rest
	const REFUNDED_AMOUNT: AssetAmount = NET_AMOUNT - CHUNK_AMOUNT;

	// Allow for one retry for good measure:
	const REFUND_BLOCK: u64 = CHUNK_1_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			insert_swaps(&[params(
				Some(DcaParameters {
					number_of_chunks: NUMBER_OF_CHUNKS,
					chunk_interval: CHUNK_INTERVAL,
				}),
				Some(SwapRefundParameters {
					refund_block: REFUND_BLOCK as u32,
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
		})
		.then_execute_at_block(CHUNK_1_BLOCK, |_| {})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Test,
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 1,
					input_amount: CHUNK_AMOUNT,
					output_amount: CHUNK_AMOUNT,
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
		})
		.then_execute_at_block(CHUNK_2_BLOCK, |_| {
			// Adjusting the swap rate, so that the second chunk fails due to FoK:
			SwapRate::set(0.5);
		})
		.then_execute_with(|_| {
			// Swap should fail after the first retry and result in a
			// refund of the remaining amount and egress of the already
			// executed amount.
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
					amount: CHUNK_AMOUNT,
					..
				}),
			);
		});
}

#[test]
fn dca_with_fok_fully_executed() {
	const CHUNK_1_BLOCK: u64 = 3;
	const CHUNK_1_RETRY_BLOCK: u64 = CHUNK_1_BLOCK + DEFAULT_SWAP_RETRY_DELAY_BLOCKS;
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
				Some(SwapRefundParameters {
					refund_block: CHUNK_2_BLOCK as u32,
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
			//
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
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: SWAP_REQUEST_ID,
					asset: OUTPUT_ASSET,
					// full amount should be egressed:
					amount: NET_AMOUNT,
					..
				}),
			);
		});
}

#[test]
fn dca_with_ccm_happy_path() {
	const CHUNK_1_BLOCK: u64 = 3;
	const CHUNK_2_BLOCK: u64 = CHUNK_1_BLOCK + CHUNK_INTERVAL as u64;

	const PRINCIPAL_AMOUNT: AssetAmount = NET_AMOUNT - GAS_BUDGET;

	const CHUNK_AMOUNT: AssetAmount = PRINCIPAL_AMOUNT / 2;

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
					output_amount: CHUNK_AMOUNT,
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
					execute_at: CHUNK_2_BLOCK,
					swap_type: SwapType::CcmGas,
					..
				}),
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
				RuntimeEvent::Swapping(Event::SwapExecuted {
					swap_request_id: SWAP_REQUEST_ID,
					swap_id: 3,
					input_amount: GAS_BUDGET,
					output_amount: GAS_BUDGET,
					..
				}),
				RuntimeEvent::Swapping(Event::SwapRequestCompleted {
					swap_request_id: SWAP_REQUEST_ID
				}),
			);

			ccm::assert_ccm_egressed(OUTPUT_ASSET, PRINCIPAL_AMOUNT)
		});
}

// TODO: once FoK is implemented for CCM, test full and partial refunds in CCM with DCA
