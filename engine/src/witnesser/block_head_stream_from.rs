use std::{ops::RangeInclusive, pin::Pin};

use futures::{stream, Future, Stream};

use crate::witnesser::BlockNumberable;

use futures::StreamExt;

use anyhow::{anyhow, Result};
use core::fmt::Display;

/// Takes a reorg-safe stream (strictly monotonically increasing block headers) and returns a stream
/// of block headers that begin at the `from_block` specified. This is necessary because when
/// querying for the head of the chain, you will get the head of the latest block the RPC knows
/// about. If latest block is *before* our `from_block` then we must wait until our RPC is at the
/// block before taking any actions. If the latest block is *after* our `from_block` then we must
/// query for the blocks before this one before yielding the "current" headers.
pub async fn block_head_stream_from<
	BlockNumber,
	HeaderStream,
	Header,
	// A closure that will get or generate the custom header type that we use for a particular
	// chain.
	GetHeaderClosure,
	HeaderFut,
>(
	from_block: BlockNumber,
	safe_head_stream: HeaderStream,
	get_header_fn: GetHeaderClosure,
	logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = Header> + Send + 'static>>>
where
	BlockNumber: PartialOrd + Display + Clone + Copy + Send + 'static,
	RangeInclusive<BlockNumber>: Iterator<Item = BlockNumber>,
	HeaderStream: Stream<Item = Header> + 'static + Send,
	Header: BlockNumberable<BlockNumber = BlockNumber> + 'static + Send,
	GetHeaderClosure: Fn(BlockNumber) -> HeaderFut + Send + 'static,
	HeaderFut: Future<Output = Result<Header>> + Send + Unpin + 'static,
{
	let mut safe_head_stream = Box::pin(safe_head_stream);
	while let Some(best_safe_block_header) = safe_head_stream.next().await {
		let best_safe_block_number = best_safe_block_header.block_number();
		// we only want to start witnessing once we reach the from_block specified
		if best_safe_block_number < from_block {
			slog::trace!(
				logger,
				"Not witnessing until block `{}` Received block `{}` from stream.",
				from_block,
				best_safe_block_number
			);
		} else {
			// our chain_head is above the from_block number

			let past_heads =
				Box::pin(stream::iter(from_block..=best_safe_block_number).then(get_header_fn));

			return Ok(Box::pin(
				stream::unfold(
					(past_heads, safe_head_stream),
					|(mut past_heads, mut safe_head_stream)| async {
						// we want to consume the past logs stream first, terminating if any of
						// these logs are an error
						if let Some(result_past_log) = past_heads.next().await {
							if let Ok(past_log) = result_past_log {
								Some((past_log, (past_heads, safe_head_stream)))
							} else {
								None
							}
						} else {
							// the past logs were consumed, now we consume the "future" logs
							safe_head_stream
								.next()
								.await
								.map(|future_log| (future_log, (past_heads, safe_head_stream)))
						}
					},
				)
				.fuse(),
			))
		}
	}
	Err(anyhow!("No events in safe head stream"))
}

#[cfg(test)]
mod tests {

	use crate::logging::test_utils::new_test_logger;

	use super::*;

	const FAILURE_BLOCK_NUMBER: u64 = 30;

	struct MockHeader {
		block_number: u64,
	}

	impl BlockNumberable for MockHeader {
		type BlockNumber = u64;

		fn block_number(&self) -> Self::BlockNumber {
			self.block_number
		}
	}

	async fn test_block_head_stream_from<HeaderStream>(
		from_block: u64,
		safe_head_stream: HeaderStream,
		logger: &slog::Logger,
	) -> Result<Pin<Box<dyn Stream<Item = MockHeader> + Send + 'static>>>
	where
		HeaderStream: Stream<Item = MockHeader> + 'static + Send,
	{
		block_head_stream_from(
			from_block,
			safe_head_stream,
			// we mock failure in the closure at a particular block number
			move |block_number| {
				Box::pin(async move {
					if block_number == FAILURE_BLOCK_NUMBER {
						Err(anyhow!("This is not the block you're looking for"))
					} else {
						Ok(MockHeader { block_number })
					}
				})
			},
			logger,
		)
		.await
	}

	fn mock_header(block_number: u64) -> MockHeader {
		MockHeader { block_number }
	}

	#[tokio::test]
	async fn stream_does_not_begin_yielding_until_at_from_block() {
		let logger = new_test_logger();

		let inner_stream_starts_at = 10;
		let from_block = 15;
		let inner_stream_ends_at = 20;

		let safe_head_stream =
			stream::iter((inner_stream_starts_at..inner_stream_ends_at).map(mock_header));

		let mut block_head_stream_from =
			test_block_head_stream_from(from_block, safe_head_stream, &logger)
				.await
				.unwrap();

		// We should only be yielding from the `from_block`
		for expected_block_number in from_block..inner_stream_ends_at {
			assert_eq!(
				block_head_stream_from.next().await.unwrap().block_number,
				expected_block_number
			);
		}

		assert!(block_head_stream_from.next().await.is_none());
	}

	#[tokio::test]
	async fn stream_goes_back_if_inner_stream_starts_ahead_of_from_block() {
		let logger = new_test_logger();

		let from_block = 10;
		let inner_stream_starts_at = 15;
		let inner_stream_ends_at = 20;

		let safe_head_stream =
			stream::iter((inner_stream_starts_at..inner_stream_ends_at).map(mock_header));

		let mut block_head_stream_from =
			test_block_head_stream_from(from_block, safe_head_stream, &logger)
				.await
				.unwrap();

		for expected_block_number in from_block..inner_stream_ends_at {
			assert_eq!(
				block_head_stream_from.next().await.unwrap().block_number,
				expected_block_number
			);
		}
		assert!(block_head_stream_from.next().await.is_none());
	}

	#[tokio::test]
	async fn stream_terminates_if_error_fetching_block_when_going_back() {
		let logger = new_test_logger();

		// choose blocks so we have to query back to block 30, which is explicitly set as a failure
		// block in the mock.
		let from_block = 27;
		let inner_stream_starts_at = 34;
		let inner_stream_ends_at = 40;

		let safe_head_stream =
			stream::iter((inner_stream_starts_at..inner_stream_ends_at).map(mock_header));

		let mut block_head_stream_from =
			test_block_head_stream_from(from_block, safe_head_stream, &logger)
				.await
				.unwrap();

		for expected_block_number in from_block..FAILURE_BLOCK_NUMBER {
			assert_eq!(
				block_head_stream_from.next().await.unwrap().block_number,
				expected_block_number
			);
		}
		assert!(block_head_stream_from.next().await.is_none());
	}
}
