//! Merges the streams from http and ws into one
//! This should:
//! - Return no duplicate blocks
//! - Skip no blocks
//! - Return the block from the fastest returning method first
//! - TODO: If one stops working, we still continue - logging that the other is faulty

use futures::{
    stream::{self, select},
    Stream,
};
use web3::types::U64;

use crate::logging::{ETH_HTTP_STREAM_RETURNED, ETH_WS_STREAM_RETURNED};

use super::{BlockHeaderable, BlockType};

use futures::StreamExt;

pub async fn merged_stream<BlockHeaderableStream, BlockHeaderableStream2>(
    safe_ws_head_stream: BlockHeaderableStream,
    safe_http_head_stream: BlockHeaderableStream2,
    logger: slog::Logger,
) -> impl Stream<Item = U64>
where
    BlockHeaderableStream: Stream<Item = BlockType> + 'static,
    BlockHeaderableStream2: Stream<Item = BlockType> + 'static,
{
    let merged_stream = Box::pin(select(safe_ws_head_stream, safe_http_head_stream));

    struct StreamState<BlockHeaderableStream> {
        merged_stream: BlockHeaderableStream,
        last_yielded_block_number: U64,
        logger: slog::Logger,
    }

    let init_data = StreamState {
        merged_stream,
        last_yielded_block_number: U64::from(0),
        logger,
    };

    Box::pin(stream::unfold(init_data, move |mut state| async move {
        loop {
            if let Some(current_item) = state.merged_stream.next().await {
                let current_item_block_number = current_item
                    .number()
                    .expect("block should have block number");

                println!("Current item block number: {}", current_item_block_number);
                if (current_item_block_number == state.last_yielded_block_number + 1)
                    // first iteration
                    || state.last_yielded_block_number == U64::from(0)
                {
                    match current_item {
                        BlockType::Ws(_) => {
                            slog::info!(
                                state.logger,
                                #ETH_WS_STREAM_RETURNED,
                                "Returning block number: {} from Websocket stream",
                                current_item_block_number,
                            )
                        }
                        BlockType::Http(_) => {
                            slog::info!(
                                state.logger,
                                #ETH_HTTP_STREAM_RETURNED,
                                "Returning block number: {} from HTTP stream",
                                current_item_block_number,
                            )
                        }
                    }
                    state.last_yielded_block_number = current_item_block_number;
                    break Some((current_item_block_number, state));
                } else {
                    continue;
                }
            } else {
                break None;
            }
        }
    }))
}

#[cfg(test)]
mod tests {

    use std::{pin::Pin, time::Duration};

    use futures::stream;

    use super::*;
    use crate::{
        eth::{
            http_observer::tests::dummy_block, safe_stream::tests::block_header, TranpsortProtocol,
        },
        logging::test_utils::{new_test_logger, new_test_logger_with_tag_cache},
    };

    // Generate a stream for each protocol, that, when selected upon, will return
    // in the order the items are passed in
    fn interleaved_streams(
        // contains the streams in the order they will return
        items: Vec<BlockType>,
    ) -> (
        // http
        impl Stream<Item = BlockType>,
        // ws
        impl Stream<Item = BlockType>,
    ) {
        assert!(items.len() > 0, "should have at least one item");

        const DELAY_DURATION_MILLIS: u64 = 10;

        let mut type_last_returned = items.first().unwrap().get_protocol();
        let mut http_items = Vec::new();
        let mut ws_items = Vec::new();
        let mut total_delay_increment = 0;

        for item in items {
            let protocol = item.get_protocol();
            // if we are returning the same, we can just go the next, since we are ordered
            let delay = Duration::from_millis(if protocol == type_last_returned {
                0
            } else {
                total_delay_increment = total_delay_increment + DELAY_DURATION_MILLIS;
                total_delay_increment
            });

            match protocol {
                TranpsortProtocol::Http => http_items.push((item, delay)),
                TranpsortProtocol::Ws => ws_items.push((item, delay)),
            };

            type_last_returned = protocol;
        }

        let delayed_stream = |items: Vec<(BlockType, Duration)>| {
            let items = items.into_iter();
            Box::pin(stream::unfold(items, |mut items| async move {
                while let Some((i, d)) = items.next() {
                    tokio::time::sleep(d).await;
                    return Some((i, items));
                }
                None
            }))
        };

        (delayed_stream(http_items), delayed_stream(ws_items))
    }

    fn num_to_block_type(block_number: u64, protocol: TranpsortProtocol) -> BlockType {
        match protocol {
            TranpsortProtocol::Http => BlockType::Http(dummy_block(block_number).unwrap().unwrap()),
            TranpsortProtocol::Ws => {
                BlockType::Ws(block_header(block_number as u8, block_number).unwrap())
            }
        }
    }

    #[tokio::test]
    async fn empty_inners_returns_none() {
        let logger = new_test_logger();
        let empty_block_headerable_ws: Pin<Box<dyn Stream<Item = BlockType>>> =
            Box::pin(stream::empty());

        let empty_block_headerable_http: Pin<Box<dyn Stream<Item = BlockType>>> =
            Box::pin(stream::empty());

        let mut merged_stream = merged_stream(
            empty_block_headerable_ws,
            empty_block_headerable_http,
            logger,
        )
        .await;

        assert!(merged_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn stream_behind_never_returns() {
        let logger = new_test_logger();

        // since these streams yield instantly, they will alternate being called
        let http_blocks: Vec<_> = (10..15)
            .into_iter()
            .map(|i| num_to_block_type(i, TranpsortProtocol::Ws))
            .collect();
        let http_stream = stream::iter(http_blocks);

        let blocks_in_front = 11..16;
        let ws_blocks: Vec<_> = blocks_in_front
            .clone()
            .into_iter()
            .map(|i| num_to_block_type(i, TranpsortProtocol::Ws))
            .collect();
        let ws_stream = stream::iter(ws_blocks);

        let mut merged_stream = merged_stream(ws_stream, http_stream, logger).await;
        for expected_block_number in blocks_in_front {
            let block_number = merged_stream.next().await.unwrap();
            println!("Here's the block number: {}", block_number);
            assert_eq!(block_number, U64::from(expected_block_number));
        }
        // we have exhausted both streams with no extra blocks to return
        assert!(merged_stream.next().await.is_none());
    }

    // #[tokio::test]
    // async fn test_interleaving_protocols() {
    // let (logger, mut tag_cache) = new_test_logger_with_tag_cache();
    // let mut items: Vec<BlockType> = Vec::new();
    // // return
    // items.push(num_to_block_type(10, TranpsortProtocol::Ws));
    // // return
    // items.push(num_to_block_type(11, TranpsortProtocol::Ws));
    // // ignore
    // items.push(num_to_block_type(10, TranpsortProtocol::Http));
    // // ignore
    // items.push(num_to_block_type(11, TranpsortProtocol::Http));
    // // return
    // items.push(num_to_block_type(12, TranpsortProtocol::Http));
    // // ignore
    // items.push(num_to_block_type(12, TranpsortProtocol::Ws));
    // // return
    // items.push(num_to_block_type(13, TranpsortProtocol::Http));
    // // ignore
    // items.push(num_to_block_type(13, TranpsortProtocol::Ws));
    // // return
    // items.push(num_to_block_type(14, TranpsortProtocol::Ws));

    //     let (http_stream, ws_stream) = interleaved_streams(items);

    //     let mut merged_stream = merged_stream(ws_stream, http_stream, logger).await;

    //     merged_stream.next().await;
    //     assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
    //     tag_cache.clear();

    //     merged_stream.next().await;
    //     assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
    //     tag_cache.clear();

    //     merged_stream.next().await;
    //     assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_RETURNED));
    //     tag_cache.clear();

    //     merged_stream.next().await;
    //     assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_RETURNED));
    //     tag_cache.clear();

    //     merged_stream.next().await;
    //     assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
    //     tag_cache.clear();
    // }
}
