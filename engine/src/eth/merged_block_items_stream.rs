use std::{cmp::Ordering, fmt::Debug, pin::Pin};

use futures::{stream, FutureExt, Stream, StreamExt};
use utilities::make_periodic_tick;

use anyhow::{bail, Result};

use crate::{
	constants::{
		ETH_FALLING_BEHIND_MARGIN_BLOCKS, ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL,
		ETH_STILL_BEHIND_LOG_INTERVAL,
	},
	eth::TransportProtocol,
	logging::{ETH_HTTP_STREAM_YIELDED, ETH_STREAM_BEHIND, ETH_WS_STREAM_YIELDED},
};

use super::{BlockWithItems, BlockWithProcessedItems};

/// Merges two streams of `BlockWithProcessedItems`. The intent of this function is to create
/// redundancy for HTTP and WS block item streams.
///
/// For a particular ETH block, this will only return the items for that block once, this includes
/// blocks without items of interest. i.e. we always return a contiguous sequence of ETH block
/// numbers (`BlockWithItems` containing this block number).
///
/// This will always yield from the protocol that reaches the next block number fastest (in practice
/// this is normally WS).
///
/// If the decoding has failed on one of these blocks, then we attempt to recover by waiting for the
/// other stream to reach that block, and see if it was successful. If we the other stream gets to
/// that point then we terminate the whole stream.
///
/// If the stream being used to recover terminates (i.e. no items left to yield) then this stream
/// will terminate.
///
/// If just one of the stream terminates, but the other is still yielding blocks (not in recovery)
/// this stream will continue to yield blocks until there is an error or it terminates.
///
/// Logging:
/// - Logs when the merged stream yields
/// - Logs when one stream is behind every [ETH_STILL_BEHIND_LOG_INTERVAL].
/// - Trace logs every block yield from both streams
///
/// Panics:
/// - If the streams passed into the merged streams first elements are different this can panic. The
///   assumption is that both streams passed in should be synced to the node at the start.
/// - If one of the input streams is not a contiguous stream. This can be ensured by preprocessing
///   any raw stream. e.g. see [ws_safe_stream.rs]
///
/// See the tests at the bottom of this file for some examples.
///
/// Developer notes:
/// - "Pulled" refers to an item taken from one of the inner streams
/// - "Yielded" refers to an item to be returned from this stream
pub async fn merged_block_items_stream<'a, BlockItemsStreamWs, BlockItemsStreamHttp, BlockItem>(
	safe_ws_block_items_stream: BlockItemsStreamWs,
	safe_http_block_items_stream: BlockItemsStreamHttp,
	logger: slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = BlockWithItems<BlockItem>> + Send + 'a>>>
where
	BlockItem: Debug + Send + Sync + 'static,
	BlockItemsStreamWs: Stream<Item = BlockWithProcessedItems<BlockItem>> + Unpin + Send + 'a,
	BlockItemsStreamHttp: Stream<Item = BlockWithProcessedItems<BlockItem>> + Unpin + Send + 'a,
{
	#[derive(Debug)]
	struct ProtocolState {
		last_block_pulled: u64,
		log_ticker: tokio::time::Interval,
		protocol: TransportProtocol,
	}
	#[derive(Debug)]
	struct MergedStreamState {
		last_block_yielded: u64,
		logger: slog::Logger,
	}

	struct StreamState<BlockItemsStreamWs: Stream, BlockItemsStreamHttp: Stream> {
		ws_state: ProtocolState,
		ws_stream: BlockItemsStreamWs,
		http_state: ProtocolState,
		http_stream: BlockItemsStreamHttp,
		merged_stream_state: MergedStreamState,
	}

	let init_state = StreamState::<BlockItemsStreamWs, BlockItemsStreamHttp> {
		ws_state: ProtocolState {
			last_block_pulled: 0,
			log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL, false),
			protocol: TransportProtocol::Ws,
		},
		ws_stream: safe_ws_block_items_stream,
		http_state: ProtocolState {
			last_block_pulled: 0,
			log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL, false),
			protocol: TransportProtocol::Http,
		},
		http_stream: safe_http_block_items_stream,
		merged_stream_state: MergedStreamState { last_block_yielded: 0, logger },
	};

	fn log_when_yielding(
		yielding_stream_state: &ProtocolState,
		non_yielding_stream_state: &ProtocolState,
		merged_stream_state: &MergedStreamState,
		yielding_block_number: u64,
	) {
		match yielding_stream_state.protocol {
			TransportProtocol::Http => {
				slog::info!(
					merged_stream_state.logger,
					#ETH_HTTP_STREAM_YIELDED,
					"ETH block {} returning from {} stream",
					yielding_block_number,
					yielding_stream_state.protocol
				);
			},
			TransportProtocol::Ws => {
				slog::info!(
					merged_stream_state.logger,
					#ETH_WS_STREAM_YIELDED,
					"ETH block {} returning from {} stream",
					yielding_block_number,
					yielding_stream_state.protocol
				);
			},
		}

		// We may be one ahead of the previously yielded block
		let blocks_behind = merged_stream_state.last_block_yielded + 1 -
			non_yielding_stream_state.last_block_pulled;

		// before we have pulled on each stream, we can't know if the other stream is behind
		if non_yielding_stream_state.last_block_pulled != 0 &&
			((non_yielding_stream_state.last_block_pulled + ETH_FALLING_BEHIND_MARGIN_BLOCKS) <=
				yielding_block_number) &&
			(blocks_behind % ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL == 0)
		{
			slog::warn!(
				merged_stream_state.logger,
				#ETH_STREAM_BEHIND,
				"ETH {} stream at block `{}` but {} stream at block `{}`",
				yielding_stream_state.protocol,
				yielding_block_number,
				non_yielding_stream_state.protocol,
				non_yielding_stream_state.last_block_pulled,
			);
		}
	}

	// Returns Error if:
	// 1. the protocol stream does not return a contiguous sequence of blocks
	// 2. the protocol streams have not started at the same block
	// 3. failure in `recover_with_other_stream`
	// When returning Ok, will return None if:
	// 1. the protocol stream is behind the next block to yield p
	async fn do_for_protocol<
		BlockItemsStream: Stream<Item = BlockWithProcessedItems<BlockItem>> + Unpin,
		BlockItem: Debug,
	>(
		merged_stream_state: &mut MergedStreamState,
		protocol_state: &mut ProtocolState,
		other_protocol_state: &mut ProtocolState,
		mut other_protocol_stream: BlockItemsStream,
		block_with_processed_items: BlockWithProcessedItems<BlockItem>,
	) -> Result<Option<BlockWithItems<BlockItem>>> {
		let next_block_to_yield = merged_stream_state.last_block_yielded + 1;
		let merged_has_yielded = merged_stream_state.last_block_yielded != 0;
		let has_pulled = protocol_state.last_block_pulled != 0;

		assert!(!has_pulled
            || (block_with_processed_items.block_number == protocol_state.last_block_pulled + 1), "ETH {} stream is expected to be a contiguous sequence of block items. Last pulled `{}`, got `{}`", protocol_state.protocol, protocol_state.last_block_pulled, block_with_processed_items.block_number);

		protocol_state.last_block_pulled = block_with_processed_items.block_number;

		let opt_block_items = if merged_has_yielded {
			if block_with_processed_items.block_number == next_block_to_yield {
				Some(block_with_processed_items)
			// if we're only one block "behind" we're not really "behind", we were just the
			// second stream polled
			} else if block_with_processed_items.block_number + 1 < next_block_to_yield {
				None
			} else if block_with_processed_items.block_number < next_block_to_yield {
				// we're behind, but we only want to log once every interval
				if protocol_state.log_ticker.tick().now_or_never().is_some() {
					slog::trace!(merged_stream_state.logger, "ETH {} stream pulled block {}. But this is behind the next block to yield of {}. Continuing...", protocol_state.protocol, block_with_processed_items.block_number, next_block_to_yield);
				}
				None
			} else {
				panic!("Input streams to merged stream started at different block numbers. This should not occur.");
			}
		} else {
			// yield
			Some(block_with_processed_items)
		};

		if let Some(block_with_processed_items) = opt_block_items {
			match block_with_processed_items.processed_block_items {
				Ok(block_items) => {
					// yield, if we are at high enough block number
					log_when_yielding(
						protocol_state,
						other_protocol_state,
						merged_stream_state,
						block_with_processed_items.block_number,
					);
					Ok(Some(BlockWithItems {
						block_number: block_with_processed_items.block_number,
						block_items,
					}))
				},
				Err(err) => {
					slog::error!(
                        merged_stream_state.logger,
                        "ETH {} stream failed to get block items for ETH block `{}`. Attempting to recover. Error: {}",
                        protocol_state.protocol,
                        block_with_processed_items.block_number,
                        err
                    );
					while let Some(block_with_processed_items) = other_protocol_stream.next().await
					{
						other_protocol_state.last_block_pulled =
							block_with_processed_items.block_number;
						match block_with_processed_items.block_number.cmp(&next_block_to_yield) {
							Ordering::Equal => {
								// we want to yield this one :)
								match block_with_processed_items.processed_block_items {
									Ok(block_items) => {
										log_when_yielding(
											other_protocol_state,
											protocol_state,
											merged_stream_state,
											block_with_processed_items.block_number,
										);
										return Ok(Some(BlockWithItems {
											block_number: block_with_processed_items.block_number,
											block_items,
										}))
									},
									Err(err) => {
										bail!("ETH {} stream failed with error, on block {} that we were recovering from: {}", other_protocol_state.protocol, block_with_processed_items.block_number, err);
									},
								}
							},
							Ordering::Less => {
								slog::trace!(merged_stream_state.logger, "ETH {} stream pulled block `{}` but still below the next block to yield of {}", other_protocol_state.protocol, block_with_processed_items.block_number, next_block_to_yield)
							},
							Ordering::Greater => {
								// This is ensured by the safe streams
								panic!(
                                    "ETH {} stream skipped blocks. Next block to yield was `{}` but got block `{}`. This should not occur",
                                    other_protocol_state.protocol,
                                    next_block_to_yield,
                                    block_with_processed_items.block_number
                                );
							},
						}
					}

					bail!(
						"ETH {} stream terminated when attempting to recover",
						other_protocol_state.protocol,
					);
				},
			}
		} else {
			Ok(None)
		}
	}

	Ok(Box::pin(stream::unfold(init_state, |mut stream_state| async move {
		loop {
			let next_clean_block_items = tokio::select! {
				Some(block_items) = stream_state.ws_stream.next() => {
					do_for_protocol(&mut stream_state.merged_stream_state, &mut stream_state.ws_state, &mut stream_state.http_state, &mut stream_state.http_stream, block_items).await
				}
				Some(block_items) = stream_state.http_stream.next() => {
					do_for_protocol(&mut stream_state.merged_stream_state, &mut stream_state.http_state, &mut stream_state.ws_state, &mut stream_state.ws_stream, block_items).await
				}
				else => break None
			};

			match next_clean_block_items {
				Ok(opt_clean_block_items) => {
					if let Some(clean_block_items) = opt_clean_block_items {
						stream_state.merged_stream_state.last_block_yielded =
							clean_block_items.block_number;
						break Some((clean_block_items, stream_state))
					}
				},
				Err(err) => {
					slog::error!(
						stream_state.merged_stream_state.logger,
						"Terminating ETH merged block stream due to error: {}",
						err
					);
					break None
				},
			}
		}
	})))
}

#[cfg(test)]
mod merged_stream_tests {

	use super::*;

	use std::time::Duration;

	use sp_core::U256;
	use utilities::assert_future_panics;

	use crate::{
		eth::{
			event::Event,
			key_manager::{ChainflipKey, KeyManagerEvent},
		},
		logging::{
			test_utils::{new_test_logger, new_test_logger_with_tag_cache},
			ETH_WS_STREAM_YIELDED,
		},
	};

	use anyhow::anyhow;

	fn make_dummy_events(log_indices: &[u8]) -> Vec<Event<KeyManagerEvent>> {
		log_indices
			.iter()
			.map(|log_index| Event::<KeyManagerEvent> {
				tx_hash: Default::default(),
				log_index: U256::from(*log_index),
				event_parameters: KeyManagerEvent::AggKeySetByAggKey {
					old_agg_key: ChainflipKey::default(),
					new_agg_key: ChainflipKey::default(),
				},
			})
			.collect()
	}

	fn block_with_ok_events_decoding(
		block_number: u64,
		log_indices: &[u8],
	) -> BlockWithProcessedItems<Event<KeyManagerEvent>> {
		BlockWithProcessedItems {
			block_number,
			processed_block_items: Ok(make_dummy_events(log_indices)),
		}
	}

	fn block_with_err_events_decoding(
		block_number: u64,
	) -> BlockWithProcessedItems<Event<KeyManagerEvent>> {
		BlockWithProcessedItems { block_number, processed_block_items: Err(anyhow!("NOOOO")) }
	}

	fn block_with_events(
		block_number: u64,
		log_indices: &[u8],
	) -> BlockWithItems<Event<KeyManagerEvent>> {
		BlockWithItems { block_number, block_items: make_dummy_events(log_indices) }
	}

	async fn test_merged_stream_interleaving(
		interleaved_blocks: Vec<(
			BlockWithProcessedItems<Event<KeyManagerEvent>>,
			TransportProtocol,
		)>,
		expected_blocks: &[(BlockWithItems<Event<KeyManagerEvent>>, TransportProtocol)],
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

			let delayed_stream =
				|blocks: Vec<(BlockWithProcessedItems<Event<KeyManagerEvent>>, Duration)>| {
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

		let (logger, mut tag_cache) = new_test_logger_with_tag_cache();

		assert_eq!(
			merged_block_items_stream(ws_stream, http_stream, logger)
				.await
				.unwrap()
				.map(move |x| {
					(x, {
						let protocol = if tag_cache.contains_tag(ETH_WS_STREAM_YIELDED) &&
							!tag_cache.contains_tag(ETH_HTTP_STREAM_YIELDED)
						{
							TransportProtocol::Ws
						} else if !tag_cache.contains_tag(ETH_WS_STREAM_YIELDED) &&
							tag_cache.contains_tag(ETH_HTTP_STREAM_YIELDED)
						{
							TransportProtocol::Http
						} else {
							panic!()
						};
						tag_cache.clear();
						protocol
					})
				})
				.collect::<Vec<_>>()
				.await,
			expected_blocks
		);
	}

	#[tokio::test]
	async fn empty_inners_returns_none() {
		assert!(merged_block_items_stream(
			Box::pin(stream::empty::<BlockWithProcessedItems<()>>()),
			Box::pin(stream::empty::<BlockWithProcessedItems<()>>()),
			new_test_logger(),
		)
		.await
		.unwrap()
		.next()
		.await
		.is_none());
	}

	#[tokio::test]
	async fn merged_does_not_return_duplicate_blocks() {
		assert_eq!(
			merged_block_items_stream(
				Box::pin(stream::iter([
					block_with_ok_events_decoding(10, &[0]),
					block_with_ok_events_decoding(11, &[]),
					block_with_ok_events_decoding(12, &[]),
					block_with_ok_events_decoding(13, &[0]),
				])),
				Box::pin(stream::iter([
					block_with_ok_events_decoding(10, &[0]),
					block_with_ok_events_decoding(11, &[]),
					block_with_ok_events_decoding(12, &[]),
					block_with_ok_events_decoding(13, &[0]),
				])),
				new_test_logger(),
			)
			.await
			.unwrap()
			.collect::<Vec<_>>()
			.await,
			&[
				block_with_events(10, &[0]),
				block_with_events(11, &[]),
				block_with_events(12, &[]),
				block_with_events(13, &[0]),
			]
		);
	}

	#[tokio::test]
	async fn merged_stream_handles_broken_stream() {
		assert_eq!(
			merged_block_items_stream(
				Box::pin(stream::empty()),
				Box::pin(stream::iter([
					block_with_ok_events_decoding(10, &[0]),
					block_with_ok_events_decoding(11, &[]),
					block_with_ok_events_decoding(12, &[]),
					block_with_ok_events_decoding(13, &[0]),
				])),
				new_test_logger(),
			)
			.await
			.unwrap()
			.collect::<Vec<_>>()
			.await,
			&[
				block_with_events(10, &[0]),
				block_with_events(11, &[]),
				block_with_events(12, &[]),
				block_with_events(13, &[0]),
			]
		);
	}

	#[tokio::test]
	async fn interleaved_streams_works_as_expected() {
		test_merged_stream_interleaving(
			vec![
				(block_with_ok_events_decoding(10, &[]), TransportProtocol::Http), // returned
				(block_with_ok_events_decoding(11, &[0]), TransportProtocol::Http), // returned
				(block_with_ok_events_decoding(10, &[]), TransportProtocol::Ws),   // ignored
				(block_with_ok_events_decoding(11, &[0]), TransportProtocol::Ws),  // ignored
				(block_with_ok_events_decoding(12, &[0]), TransportProtocol::Ws),  // returned
				(block_with_ok_events_decoding(12, &[0]), TransportProtocol::Http), // ignored
				(block_with_ok_events_decoding(13, &[]), TransportProtocol::Ws),   // returned
				(block_with_ok_events_decoding(14, &[]), TransportProtocol::Ws),   // returned
				(block_with_ok_events_decoding(13, &[]), TransportProtocol::Http), // ignored
				(block_with_ok_events_decoding(14, &[]), TransportProtocol::Http), // ignored
				(block_with_ok_events_decoding(15, &[0]), TransportProtocol::Ws),  // returned
				(block_with_ok_events_decoding(15, &[0]), TransportProtocol::Http), // ignored
			],
			&[
				(block_with_events(10, &[]), TransportProtocol::Http),
				(block_with_events(11, &[0]), TransportProtocol::Http),
				(block_with_events(12, &[0]), TransportProtocol::Ws),
				(block_with_events(13, &[]), TransportProtocol::Ws),
				(block_with_events(14, &[]), TransportProtocol::Ws),
				(block_with_events(15, &[0]), TransportProtocol::Ws),
			],
		)
		.await;
	}

	#[tokio::test]
	async fn merged_stream_notifies_once_every_x_blocks_when_one_falls_behind() {
		let (logger, tag_cache) = new_test_logger_with_tag_cache();

		let ws_range = 10..54;

		assert!(Iterator::eq(
			merged_block_items_stream(
				stream::iter(ws_range.clone().map(|n| block_with_ok_events_decoding(n, &[0]))),
				stream::iter([block_with_ok_events_decoding(10, &[0])]),
				logger
			)
			.await
			.unwrap()
			.collect::<Vec<_>>()
			.await
			.into_iter(),
			ws_range.map(|i| block_with_events(i, &[0]))
		));
		assert_eq!(tag_cache.get_tag_count(ETH_STREAM_BEHIND), 4);
	}

	#[tokio::test]
	async fn merged_stream_panics_if_a_stream_moves_backwards() {
		let mut stream = merged_block_items_stream(
			Box::pin(stream::iter([
				block_with_ok_events_decoding(12, &[0]),
				block_with_ok_events_decoding(13, &[]),
				block_with_ok_events_decoding(14, &[2]),
				// We jump back here
				block_with_ok_events_decoding(13, &[]),
				block_with_ok_events_decoding(15, &[]),
				block_with_ok_events_decoding(16, &[0]),
			])),
			Box::pin(stream::iter([
				block_with_ok_events_decoding(12, &[0]),
				block_with_ok_events_decoding(13, &[]),
				block_with_ok_events_decoding(14, &[2]),
				// We jump back here
				block_with_ok_events_decoding(13, &[]),
				block_with_ok_events_decoding(15, &[]),
				block_with_ok_events_decoding(16, &[0]),
			])),
			new_test_logger(),
		)
		.await
		.unwrap();

		stream.next().await.unwrap();
		stream.next().await.unwrap();
		stream.next().await.unwrap();
		assert_future_panics!(stream.next());
	}

	#[tokio::test]
	async fn merged_stream_recovers_when_one_stream_errors_and_other_catches_up_with_success() {
		test_merged_stream_interleaving(
			vec![
				(block_with_ok_events_decoding(5, &[]), TransportProtocol::Http),
				(block_with_ok_events_decoding(6, &[0]), TransportProtocol::Http),
				(block_with_ok_events_decoding(7, &[]), TransportProtocol::Http),
				(block_with_ok_events_decoding(8, &[]), TransportProtocol::Http),
				(block_with_ok_events_decoding(9, &[]), TransportProtocol::Http),
				// we had some events, but they are an error
				(block_with_err_events_decoding(10), TransportProtocol::Http),
				// so now we should enter recovery on the websockets stream
				(block_with_ok_events_decoding(5, &[]), TransportProtocol::Ws),
				(block_with_ok_events_decoding(6, &[0]), TransportProtocol::Ws),
				(block_with_ok_events_decoding(7, &[]), TransportProtocol::Ws),
				(block_with_ok_events_decoding(8, &[]), TransportProtocol::Ws),
				(block_with_ok_events_decoding(9, &[]), TransportProtocol::Ws),
				(block_with_ok_events_decoding(10, &[4]), TransportProtocol::Ws),
			],
			&[
				(block_with_events(5, &[]), TransportProtocol::Http),
				(block_with_events(6, &[0]), TransportProtocol::Http),
				(block_with_events(7, &[]), TransportProtocol::Http),
				(block_with_events(8, &[]), TransportProtocol::Http),
				(block_with_events(9, &[]), TransportProtocol::Http),
				(block_with_events(10, &[4]), TransportProtocol::Ws),
			],
		)
		.await;
	}

	#[tokio::test]
	async fn merged_stream_exits_when_both_streams_have_error_events_for_a_block() {
		assert_eq!(
			merged_block_items_stream(
				Box::pin(stream::iter([
					block_with_ok_events_decoding(11, &[0]),
					block_with_err_events_decoding(12),
				])),
				Box::pin(stream::iter([
					block_with_ok_events_decoding(11, &[0]),
					block_with_err_events_decoding(12),
				])),
				new_test_logger()
			)
			.await
			.unwrap()
			.collect::<Vec<_>>()
			.await,
			&[block_with_events(11, &[0])]
		);
	}
}
