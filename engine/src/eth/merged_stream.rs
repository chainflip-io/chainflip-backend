//! Merges the streams from http and ws into one
//! This should:
//! - Return no duplicate blocks
//! - Skip no blocks
//! - Return the block from the fastest returning method first
//! - If one stops working, we still continue - logging that the other is faulty

use std::pin::Pin;

use futures::{
    stream::{self, repeat, select},
    Stream,
};
use web3::types::U64;

use crate::eth::TranpsortProtocol;

use super::BlockHeaderable;

type DynBlockHeader = Box<dyn BlockHeaderable>;

use futures::StreamExt;

pub async fn merged_stream<BlockHeaderableStream>(
    safe_ws_head_stream: BlockHeaderableStream,
    safe_http_head_stream: BlockHeaderableStream,
    logger: slog::Logger,
    // TODO: return type
) -> Pin<Box<dyn Stream<Item = U64>>>
where
    // EthBlockHeader: BlockHeaderable,
    BlockHeaderableStream: Stream<Item = DynBlockHeader> + 'static,
{
    // TODO: Add the zip stuff later for logging / tracking faults
    let safe_ws_head_stream = safe_ws_head_stream.zip(repeat(TranpsortProtocol::Ws));
    let safe_http_head_stream = safe_http_head_stream.zip(repeat(TranpsortProtocol::Http));
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

    let merged_stream = stream::unfold(init_data, move |mut state| async move {
        loop {
            if let Some((current_item, protocol)) = state.merged_stream.next().await {
                let current_item_block_number = current_item
                    .number()
                    .expect("block should have block number");

                if current_item_block_number > state.last_yielded_block_number {
                    slog::info!(
                        state.logger,
                        "Returning block number: {} from {} stream",
                        current_item_block_number,
                        protocol
                    );
                    state.last_yielded_block_number = current_item_block_number;
                    break Some((current_item_block_number, state));
                } else {
                    continue;
                }
            } else {
                break None;
            }
        }
    });

    Box::pin(merged_stream)
}

#[cfg(test)]
mod tests {

    use futures::stream;

    use super::*;
    use crate::{
        eth::{http_observer::tests::dummy_block, safe_stream::tests::block_header},
        logging::test_utils::new_test_logger,
    };

    #[tokio::test]
    async fn empty_inners_returns_none() {
        let logger = new_test_logger();
        let empty_block_headerable_ws: Pin<Box<dyn Stream<Item = Box<dyn BlockHeaderable>>>> =
            Box::pin(stream::empty());

        let empty_block_headerable_http: Pin<Box<dyn Stream<Item = Box<dyn BlockHeaderable>>>> =
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
            .map(|i| {
                let http_block: Box<dyn BlockHeaderable> =
                    Box::new(dummy_block(i).unwrap().unwrap());
                http_block
            })
            .collect();
        let http_stream = stream::iter(http_blocks);

        let ws_blocks: Vec<_> = (11..16)
            .into_iter()
            .map(|i| {
                let ws_block_header: Box<dyn BlockHeaderable> =
                    Box::new(block_header(i, i.into()).unwrap());
                ws_block_header
            })
            .collect();
        let ws_stream = stream::iter(ws_blocks);

        let mut merged_stream = merged_stream(ws_stream, http_stream, logger).await;
        for expected_block_number in 11..16 {
            let block_number = merged_stream.next().await.unwrap();
            assert_eq!(block_number, U64::from(expected_block_number));
        }
        // we have exhausted both streams with no extra blocks to return
        assert!(merged_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_the_mother_fucker() {
        let logger = new_test_logger();
        // TODO use zip instead of iter tuple ? not sure if there's a difference
        let http_block: Box<dyn BlockHeaderable> = Box::new(dummy_block(10).unwrap().unwrap());
        let http_stream = stream::iter([http_block]);

        let ws_block_header: Box<dyn BlockHeaderable> = Box::new(block_header(0, 10).unwrap());
        let ws_stream = stream::iter([ws_block_header]);

        let mut merged_stream: Pin<Box<dyn Stream<Item = U64>>> =
            merged_stream(http_stream, ws_stream, logger).await;

        while let Some(item) = merged_stream.next().await {
            println!("Here's the item mother fucker: {:?}", item);
        }
    }
}
