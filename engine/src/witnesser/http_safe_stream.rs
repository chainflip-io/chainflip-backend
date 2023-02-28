use std::{ops::Add, time::Duration};

use futures::{stream, Stream};
use tokio::time::Interval;
use tracing::{info_span, Instrument};
use utilities::make_periodic_tick;

pub const HTTP_POLL_INTERVAL: Duration = Duration::from_secs(4);

use crate::witnesser::LatestBlockNumber;

use num_traits::CheckedSub;

use futures::StreamExt;

use crate::retry_rpc_until_success;

/// Uses a HTTP RPC to poll for the latest block number.
/// It produces a continuous stream of blocks, starting from the latest block number
/// minus the safety margin. The safety margin is the number of blocks that must occur after
/// a particular block before it is considered "safe" from chain re-orgs.
pub async fn safe_polling_http_head_stream<BlockNumber, HttpRpc>(
	http_rpc: HttpRpc,
	poll_interval: Duration,
	safety_margin: BlockNumber,
) -> impl Stream<Item = BlockNumber>
where
	BlockNumber: CheckedSub + Add<Output = BlockNumber> + PartialOrd + From<u64> + Clone + Copy,
	HttpRpc: LatestBlockNumber<BlockNumber = BlockNumber> + Send + 'static,
{
	struct StreamState<HttpRpc, BlockNumber> {
		option_last_block_yielded: Option<BlockNumber>,
		option_last_head_fetched: Option<BlockNumber>,
		http_rpc: HttpRpc,
		poll_interval: Interval,
	}

	let init_state = StreamState {
		option_last_block_yielded: None,
		option_last_head_fetched: None,
		http_rpc,
		poll_interval: make_periodic_tick(poll_interval, false),
	};

	Box::pin(
		stream::unfold(init_state, move |mut state| {
			async move {
				let StreamState {
					option_last_block_yielded,
					option_last_head_fetched,
					http_rpc,
					poll_interval,
				} = &mut state;

				let last_head_fetched = if let Some(last_head_fetched) = option_last_head_fetched {
					last_head_fetched
				} else {
					option_last_head_fetched.insert(retry_rpc_until_success!(
						http_rpc.latest_block_number(),
						poll_interval
					))
				};

				// Only request the latest block number if we are out of blocks to yield
				while {
					if let Some(last_block_yielded) = option_last_block_yielded {
						*last_head_fetched <= *last_block_yielded + safety_margin
					} else {
						*last_head_fetched < safety_margin
					}
				} {
					poll_interval.tick().await;
					let unsafe_block_number =
						retry_rpc_until_success!(http_rpc.latest_block_number(), poll_interval);

					// Fetched unsafe_block_number is more than `safety_margin` blocks behind the
					// last fetched ETH block number (last_head_fetched)
					if unsafe_block_number + safety_margin < *last_head_fetched {
						return None
					}

					*last_head_fetched = unsafe_block_number;
				}

				let next_block_to_yield =
					if let Some(last_block_yielded) = option_last_block_yielded {
						// the last block yielded was safe, so the next is +1
						*last_block_yielded + 1.into()
					} else {
						last_head_fetched.checked_sub(&safety_margin).unwrap()
					};

				*option_last_block_yielded = Some(next_block_to_yield);
				Some((next_block_to_yield, state))
			}
			.instrument(info_span!("HTTPSafeStream"))
		})
		.fuse(),
	)
}

#[cfg(test)]
pub mod tests {

	use futures::StreamExt;
	use mockall::Sequence;

	use super::*;

	// in tests, this can be instant
	const TEST_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(1);

	use crate::{constants::ETH_BLOCK_SAFETY_MARGIN, eth::rpc::mocks::MockEthHttpRpcClient};

	use anyhow::anyhow;

	#[tokio::test]
	async fn returns_best_safe_block_immediately() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let block_number = 10;
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.returning(move || Ok(block_number));

		const SAFETY_MARGIN: u64 = 4u64;

		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			SAFETY_MARGIN,
		)
		.await;
		let expected_returned_block_number = block_number - SAFETY_MARGIN;
		assert_eq!(stream.next().await.unwrap(), expected_returned_block_number);
	}

	#[tokio::test]
	async fn does_not_return_until_chain_head_is_beyond_safety_margin() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let mut seq = Sequence::new();

		let range = 1..=ETH_BLOCK_SAFETY_MARGIN;
		for n in range {
			mock_http_rpc_client
				.expect_latest_block_number()
				.times(1)
				.in_sequence(&mut seq)
				.returning(move || Ok(n));
		}

		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			ETH_BLOCK_SAFETY_MARGIN,
		)
		.await;
		assert_eq!(stream.next().await.unwrap(), 0);
	}

	#[tokio::test]
	async fn does_not_return_block_until_progress() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let mut seq = Sequence::new();

		let first_block_number = 10;
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Ok(first_block_number));

		// We keep getting block 10 when querying for block number
		// we only want to progress once we have a new block number
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(10)
			.in_sequence(&mut seq)
			.returning(move || Ok(first_block_number));

		// the eth chain has progressed by 1...
		let next_block_number = first_block_number + 1;
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Ok(next_block_number));

		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			ETH_BLOCK_SAFETY_MARGIN,
		)
		.await;
		let expected_first_returned_block_number = first_block_number - ETH_BLOCK_SAFETY_MARGIN;
		assert_eq!(stream.next().await.unwrap(), expected_first_returned_block_number);
		let expected_next_returned_block_number = next_block_number - ETH_BLOCK_SAFETY_MARGIN;
		assert_eq!(stream.next().await.unwrap(), expected_next_returned_block_number);
	}

	#[tokio::test]
	async fn catches_up_if_polling_skipped_a_block_number() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let mut seq = Sequence::new();

		let first_block_number = 10;
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Ok(first_block_number));

		// if we skip blocks, we should catch up by fetching the logs from the blocks
		// we skipped
		let num_skipped_blocks = 4;
		let next_block_number = first_block_number + num_skipped_blocks;
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Ok(next_block_number));

		let skipped_range = (first_block_number + 1)..(first_block_number + num_skipped_blocks);

		const SAFETY_MARGIN: u64 = 4u64;

		// first block should come in as expected
		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			SAFETY_MARGIN,
		)
		.await;
		let expected_first_returned_block_number = first_block_number - SAFETY_MARGIN;
		assert_eq!(stream.next().await.unwrap(), expected_first_returned_block_number);

		// we should get all the skipped blocks next (that are within the safety margin)
		for n in skipped_range {
			let expected_skipped_block_number = n - SAFETY_MARGIN;
			assert_eq!(stream.next().await.unwrap(), expected_skipped_block_number);
		}
	}

	#[tokio::test]
	async fn if_block_number_decreases_from_last_request_wait_until_back_to_prev_latest_block() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let mut seq = Sequence::new();

		const SAFETY_MARGIN: u64 = 4;

		let first_block_number = 10;
		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Ok(first_block_number));

		let first_safe_block_number = first_block_number - SAFETY_MARGIN;

		let back_to_block_number = first_block_number - 2;

		// We want to return the one after the first one we have already returned
		for n in back_to_block_number..=first_block_number + 1 {
			mock_http_rpc_client
				.expect_latest_block_number()
				.times(1)
				.in_sequence(&mut seq)
				.returning(move || Ok(n));
		}

		// This is the next block that should be yielded. It shouldn't matter to the caller of
		// .next() if the chain head has decreased due to sync / reorgs
		let next_safe_block_number = first_safe_block_number + 1;

		// first block should come in as expected
		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			SAFETY_MARGIN,
		)
		.await;
		assert_eq!(stream.next().await.unwrap(), first_safe_block_number);

		// We do not want any repeat blocks, we will just wait until we can return the next safe
		// block, after the one we've already returned
		assert_eq!(stream.next().await.unwrap(), next_safe_block_number);
	}

	#[tokio::test]
	async fn if_block_numbers_increment_by_one_progresses_at_block_margin() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let mut seq = Sequence::new();

		let block_range = 10..20;

		for block_number in block_range.clone() {
			mock_http_rpc_client
				.expect_latest_block_number()
				.times(1)
				.in_sequence(&mut seq)
				.returning(move || Ok(block_number));
		}

		const SAFETY_MARGIN: u64 = 4;

		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			SAFETY_MARGIN,
		)
		.await;

		for block_number in block_range {
			if let Some(block_number_stream) = stream.next().await {
				assert_eq!(block_number_stream, block_number - SAFETY_MARGIN);
			};
		}
	}

	#[tokio::test]
	async fn stalls_on_bad_block_number_poll() {
		let mut mock_http_rpc_client = MockEthHttpRpcClient::new();

		let mut seq = Sequence::new();

		let end_of_successful_block_range = 13;
		let block_range = 10..end_of_successful_block_range;

		for block_number in block_range.clone() {
			mock_http_rpc_client
				.expect_latest_block_number()
				.times(1)
				.in_sequence(&mut seq)
				.returning(move || Ok(block_number));
		}

		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Err(anyhow!("Failed to get block number, you fool")));

		mock_http_rpc_client
			.expect_latest_block_number()
			.times(1)
			.in_sequence(&mut seq)
			.returning(move || Ok(end_of_successful_block_range));

		const SAFETY_MARGIN: u64 = 4;

		let mut stream = safe_polling_http_head_stream(
			mock_http_rpc_client,
			TEST_HTTP_POLL_INTERVAL,
			SAFETY_MARGIN,
		)
		.await;

		for block_number in block_range {
			if let Some(block_number_from_stream) = stream.next().await {
				assert_eq!(block_number_from_stream, block_number - SAFETY_MARGIN);
			};
		}

		assert_eq!(stream.next().await.unwrap(), end_of_successful_block_range - SAFETY_MARGIN);
	}
}
