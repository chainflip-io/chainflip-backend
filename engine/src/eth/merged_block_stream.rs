use std::fmt::Debug;

use futures::{stream, FutureExt, Stream, StreamExt};
use tracing::{debug, trace, warn};
use utilities::make_periodic_tick;

use crate::{
	constants::{
		ETH_FALLING_BEHIND_MARGIN_BLOCKS, ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL,
		ETH_STILL_BEHIND_LOG_INTERVAL,
	},
	eth::TransportProtocol,
	logging::ETH_STREAM_BEHIND,
	witnesser::BlockNumberable,
};

/// Merges two streams of blocks. The intent of this function is to create
/// redundancy for HTTP and WS block item streams.
///
/// Panics:
/// - If the streams passed into the merged streams first elements are different this can panic. The
///   assumption is that both streams passed in should be synced to the node at the start. This can
///   be ensured by preprocessing a raw stream with something like `block_head_stream_from`.
/// - If one of the input streams is not a contiguous stream. This can be ensured by preprocessing
///   any raw stream. e.g. see [ws_safe_stream.rs]
///
/// For a particular ETH block, this will only return the block once, this includes
/// blocks without items of interest. i.e. we always return a contiguous sequence of ETH block
/// numbers.
///
/// This will always yield from the protocol that reaches the next block number fastest (in practice
/// this is normally WS).
///
/// If just one of the stream terminates, but the other is still yielding blocks
/// this stream will continue to yield blocks until there is an error or it terminates.
///
/// Logging:
/// - Logs when the merged stream yields
/// - Logs when one stream is behind every [ETH_STILL_BEHIND_LOG_INTERVAL].
/// - Trace logs every block yield from both streams
///
/// See the tests at the bottom of this file for some examples.
///
/// Developer notes:
/// - "Pulled" refers to an item taken from one of the inner streams
/// - "Yielded" refers to an item to be returned from this stream
pub async fn merged_block_stream<'a, Block, BlockHeaderStreamWs, BlockHeaderStreamHttp>(
	safe_ws_block_items_stream: BlockHeaderStreamWs,
	safe_http_block_items_stream: BlockHeaderStreamHttp,
) -> impl Stream<Item = (Block, TransportProtocol)> + Send + 'a
where
	Block: BlockNumberable + Send + 'a,
	BlockHeaderStreamWs: Stream<Item = Block> + Unpin + Send + 'a,
	BlockHeaderStreamHttp: Stream<Item = Block> + Unpin + Send + 'a,
{
	#[derive(Debug)]
	struct ProtocolState {
		last_block_pulled: Option<u64>,
		log_ticker: tokio::time::Interval,
		protocol: TransportProtocol,
	}
	#[derive(Debug)]
	struct MergedStreamState {
		last_block_yielded: Option<u64>,
	}

	struct StreamState<BlockItemsStreamWs: Stream, BlockItemsStreamHttp: Stream> {
		ws_state: ProtocolState,
		ws_stream: BlockItemsStreamWs,
		http_state: ProtocolState,
		http_stream: BlockItemsStreamHttp,
		merged_stream_state: MergedStreamState,
	}

	let init_state = StreamState::<BlockHeaderStreamWs, BlockHeaderStreamHttp> {
		ws_state: ProtocolState {
			last_block_pulled: None,
			log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL, false),
			protocol: TransportProtocol::Ws,
		},
		ws_stream: safe_ws_block_items_stream,
		http_state: ProtocolState {
			last_block_pulled: None,
			log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL, false),
			protocol: TransportProtocol::Http,
		},
		http_stream: safe_http_block_items_stream,
		merged_stream_state: MergedStreamState { last_block_yielded: None },
	};

	fn log_when_yielding(
		yielding_stream_state: &ProtocolState,
		non_yielding_stream_state: &ProtocolState,
		yielding_block_number: u64,
	) {
		match yielding_stream_state.protocol {
			TransportProtocol::Http => {
				debug!(
					"ETH block {yielding_block_number} returning from {} stream",
					yielding_stream_state.protocol
				);
			},
			TransportProtocol::Ws => {
				debug!(
					"ETH block {yielding_block_number} returning from {} stream",
					yielding_stream_state.protocol
				);
			},
		}

		if let Some(non_yielding_last_pulled) = non_yielding_stream_state.last_block_pulled {
			let blocks_behind = yielding_block_number - non_yielding_last_pulled;

			if ((non_yielding_last_pulled + ETH_FALLING_BEHIND_MARGIN_BLOCKS) <=
				yielding_block_number) &&
				(blocks_behind % ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL == 0)
			{
				warn!(
					tag = ETH_STREAM_BEHIND,
					"ETH {} stream at block `{yielding_block_number}` but {} stream at block `{non_yielding_last_pulled}`",
					yielding_stream_state.protocol,
					non_yielding_stream_state.protocol,
				);
			}
		}
	}

	// When returning Ok, will return None if the protocol
	// stream is behind the next block to yield
	async fn on_block_for_protocol<Block: BlockNumberable>(
		merged_stream_state: &mut MergedStreamState,
		protocol_state: &mut ProtocolState,
		other_protocol_state: &ProtocolState,
		block: Block,
	) -> Option<(Block, TransportProtocol)> {
		let block_number: u64 = block.block_number().into();

		if let Some(last_pulled) = protocol_state.last_block_pulled {
			assert_eq!(
				block_number, last_pulled + 1,
				"ETH {} stream is expected to be a contiguous sequence of block items. Last pulled `{last_pulled}`, got `{block_number}`",
				protocol_state.protocol,
			);
		}

		protocol_state.last_block_pulled = Some(block_number);

		let opt_block_header = if let Some(last_block_yielded) =
			merged_stream_state.last_block_yielded
		{
			let next_block_to_yield = last_block_yielded + 1;
			if block_number == next_block_to_yield {
				Some(block)
			// if we're only one block behind we're not really "behind", we were just the
			// second stream polled
			} else if block_number + 1 < next_block_to_yield {
				None
			} else if block_number < next_block_to_yield {
				// we're behind, but we only want to log once every interval
				if protocol_state.log_ticker.tick().now_or_never().is_some() {
					trace!( "ETH {} stream pulled block {block_number}. But this is behind the next block to yield of {next_block_to_yield}. Continuing...", protocol_state.protocol);
				}
				None
			} else {
				panic!("Input streams to merged stream started at different block numbers. This should not occur.");
			}
		} else {
			// yield
			Some(block)
		}.map(|block| (block, protocol_state.protocol));

		if opt_block_header.is_some() {
			log_when_yielding(protocol_state, other_protocol_state, block_number);
		}

		opt_block_header
	}

	Box::pin(stream::unfold(init_state, |mut stream_state| async move {
		let StreamState {
			ws_state, ws_stream, http_state, http_stream, merged_stream_state, ..
		} = &mut stream_state;

		loop {
			if let Some((block, protocol)) = tokio::select! {
				Some(block_header) = ws_stream.next() => {
					on_block_for_protocol(merged_stream_state, ws_state, http_state, block_header).await
				}
				Some(block_header) = http_stream.next() => {
					on_block_for_protocol(merged_stream_state, http_state, ws_state, block_header).await
				}
				else => break None
			} {
				stream_state.merged_stream_state.last_block_yielded =
					Some(block.block_number().into());
				break Some(((block, protocol), stream_state))
			}
		}
	}))
}

#[cfg(test)]
mod merged_stream_tests {

	use super::*;

	use std::time::Duration;

	use utilities::assert_future_panics;

	async fn test_merged_stream_interleaving<Block: BlockNumberable + PartialEq + Debug + Send>(
		interleaved_blocks: Vec<(Block, TransportProtocol)>,
		expected_blocks: &[(Block, TransportProtocol)],
	) {
		// Generate a stream for each protocol, that, when selected upon, will return
		// in the order the blocks are passed in
		// This is useful to test more "real world" scenarios, as stream::iter will always
		// immediately yield, therefore blocks will always be pealed off the streams
		// alternatingly
		let (ws_stream, http_stream) = {
			assert!(!interleaved_blocks.is_empty(), "should have at least one item");

			const DELAY_DURATION_MILLIS: u64 = 50;

			let mut protocol_last_returned = interleaved_blocks.first().unwrap().1;
			let mut http_blocks = Vec::new();
			let mut ws_blocks = Vec::new();
			let mut total_delay_increment = 0;

			for (block, protocol) in interleaved_blocks {
				// if we are returning the same, we can just go the next, since we are ordered
				let delay = Duration::from_millis(if protocol == protocol_last_returned {
					0
				} else {
					total_delay_increment += DELAY_DURATION_MILLIS;
					total_delay_increment
				});

				match protocol {
					TransportProtocol::Http => http_blocks.push((block, delay)),
					TransportProtocol::Ws => ws_blocks.push((block, delay)),
				};

				protocol_last_returned = protocol;
			}

			let delayed_stream = |blocks: Vec<(_, Duration)>| {
				let blocks = blocks.into_iter();
				Box::pin(
					stream::unfold(blocks, |mut blocks| async move {
						if let Some((i, d)) = blocks.next() {
							tokio::time::sleep(d).await;
							Some((i, blocks))
						} else {
							None
						}
					})
					.fuse(),
				)
			};

			(delayed_stream(ws_blocks), delayed_stream(http_blocks))
		};

		assert_eq!(
			merged_block_stream(ws_stream, http_stream).await.collect::<Vec<_>>().await,
			expected_blocks
		);
	}

	#[tokio::test]
	async fn empty_inners_returns_none() {
		assert!(merged_block_stream(
			Box::pin(stream::empty::<u64>()),
			Box::pin(stream::empty::<u64>()),
		)
		.await
		.next()
		.await
		.is_none());
	}

	#[tokio::test]
	async fn merged_does_not_return_duplicate_blocks() {
		assert_eq!(
			merged_block_stream(
				Box::pin(stream::iter([10, 11, 12, 13])),
				Box::pin(stream::iter([10, 11, 12, 13])),
			)
			.await
			.collect::<(Vec<_>, Vec<_>)>()
			.await
			.0,
			&[10, 11, 12, 13]
		);
	}

	#[tokio::test]
	async fn merged_block_stream_starts_from_0_functions_as_expected() {
		assert_eq!(
			merged_block_stream(
				Box::pin(stream::iter([0, 1, 2, 3])),
				Box::pin(stream::iter([0, 1, 2, 3])),
			)
			.await
			.collect::<(Vec<_>, Vec<_>)>()
			.await
			.0,
			&[0, 1, 2, 3]
		);
	}

	#[tokio::test]
	async fn merged_stream_handles_broken_stream() {
		assert_eq!(
			merged_block_stream(
				Box::pin(stream::empty()),
				Box::pin(stream::iter([10, 11, 12, 13])),
			)
			.await
			.collect::<(Vec<_>, Vec<_>)>()
			.await
			.0,
			&[10, 11, 12, 13]
		);
	}

	#[tokio::test]
	async fn interleaved_streams_works_as_expected() {
		test_merged_stream_interleaving(
			vec![
				(10, TransportProtocol::Http), // returned
				(11, TransportProtocol::Http), // returned
				(10, TransportProtocol::Ws),   // ignored
				(11, TransportProtocol::Ws),   // ignored
				(12, TransportProtocol::Ws),   // returned
				(12, TransportProtocol::Http), // ignored
				(13, TransportProtocol::Ws),   // returned
				(14, TransportProtocol::Ws),   // returned
				(13, TransportProtocol::Http), // ignored
				(14, TransportProtocol::Http), // ignored
				(15, TransportProtocol::Ws),   // returned
				(15, TransportProtocol::Http), // ignored
			],
			&[
				(10, TransportProtocol::Http),
				(11, TransportProtocol::Http),
				(12, TransportProtocol::Ws),
				(13, TransportProtocol::Ws),
				(14, TransportProtocol::Ws),
				(15, TransportProtocol::Ws),
			],
		)
		.await;
	}

	#[tokio::test]
	async fn merged_stream_panics_if_a_stream_moves_backwards() {
		let mut stream = merged_block_stream(
			Box::pin(stream::iter([
				12, 13, 14, // We jump back here
				13, 15, 16,
			])),
			Box::pin(stream::iter([12, 13, 14, 13, 15, 16])),
		)
		.await;

		stream.next().await.unwrap();
		stream.next().await.unwrap();
		stream.next().await.unwrap();
		assert_future_panics!(stream.next());
	}

	#[tokio::test]
	async fn merged_stream_recovers_when_one_stream_errors_and_other_catches_up_with_success() {
		test_merged_stream_interleaving(
			vec![
				(5, TransportProtocol::Http),
				(6, TransportProtocol::Http),
				(7, TransportProtocol::Http),
				(8, TransportProtocol::Http),
				(9, TransportProtocol::Http),
				(5, TransportProtocol::Ws),
				(6, TransportProtocol::Ws),
				(7, TransportProtocol::Ws),
				(8, TransportProtocol::Ws),
				(9, TransportProtocol::Ws),
				(10, TransportProtocol::Ws),
			],
			&[
				(5, TransportProtocol::Http),
				(6, TransportProtocol::Http),
				(7, TransportProtocol::Http),
				(8, TransportProtocol::Http),
				(9, TransportProtocol::Http),
				(10, TransportProtocol::Ws),
			],
		)
		.await;
	}
}
