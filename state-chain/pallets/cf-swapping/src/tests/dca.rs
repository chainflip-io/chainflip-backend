use super::*;

const INPUT_AMOUNT: AssetAmount = 40;

#[test]
fn dca_happy_path() {
	const SWAPS_ADDED_BLOCK: u64 = 1;
	const CHUNK_1_SCHEDULED_FOR_BLOCK: u64 = 3;
	const CHUNK_2_SCHEDULED_FOR_BLOCK: u64 = 5;

	const INPUT_ASSET: Asset = Asset::Eth;
	const OUTPUT_ASSET: Asset = Asset::Usdc;

	const CHUNK_AMOUNT: AssetAmount = INPUT_AMOUNT / 2;

	// 1:1 swap ratio
	const TOTAL_OUTPUT_AMOUNT: AssetAmount = INPUT_AMOUNT;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), SWAPS_ADDED_BLOCK);

			insert_swaps(&[TestSwapParams {
				input_asset: INPUT_ASSET,
				output_asset: OUTPUT_ASSET,
				input_amount: INPUT_AMOUNT,
				refund_params: None,
				dca_params: Some(DCAParameters { number_of_chunks: 2, swap_interval: 2 }),
				output_address: ForeignChainAddress::Eth([1; 20].into()),
			}]);

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
					execute_at: CHUNK_1_SCHEDULED_FOR_BLOCK,
					..
				})
			);
		})
		.then_execute_at_block(CHUNK_1_SCHEDULED_FOR_BLOCK, |_| {})
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
					execute_at: CHUNK_2_SCHEDULED_FOR_BLOCK,
					..
				})
			);
		})
		.then_execute_at_block(CHUNK_2_SCHEDULED_FOR_BLOCK, |_| {})
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
				RuntimeEvent::Swapping(Event::SwapRequestCompleted { swap_request_id: 1 }),
				RuntimeEvent::Swapping(Event::SwapEgressScheduled {
					swap_request_id: 1,
					amount: TOTAL_OUTPUT_AMOUNT,
					..
				})
			);
		});
}
