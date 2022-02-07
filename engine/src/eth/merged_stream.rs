//! Merges the streams from http and ws into one
//! This should:
//! - Return no duplicate blocks
//! - Skip no blocks
//! - Return the block from the fastest returning method first
//! - If one stops working, we still continue - logging that the other is faulty

use std::pin::Pin;

use futures::{stream::select, Stream};
use web3::types::BlockNumber;

use super::BlockHeaderable;

use futures::StreamExt;

pub async fn merged_stream<BlockHeaderableStream>(
    safe_ws_head_stream: BlockHeaderableStream,
    safe_http_head_stream: BlockHeaderableStream,
    // TODO: return type
) -> Pin<Box<dyn Stream<Item = BlockNumber>>>
where
    // EthBlockHeader: BlockHeaderable,
    BlockHeaderableStream: Stream<Item = Box<dyn BlockHeaderable>> + 'static,
{
    // TODO: Add the zip stuff later for logging / tracking faults
    // .zip(repeat(TranpsortProtocol::Ws));
    // .zip(repeat(TranpsortProtocol::Http));
    let merged_stream = select(safe_ws_head_stream, safe_http_head_stream);

    Box::pin(merged_stream.map(|block| block.number().unwrap().into()))
}

#[cfg(test)]
mod tests {

    use futures::stream;

    use super::*;
    use crate::eth::{
        http_observer::tests::dummy_block, safe_stream::tests::block_header, TranpsortProtocol,
    };

    #[tokio::test]
    async fn test_the_mother_fucker() {
        // TODO use zip instead of iter tuple ? not sure if there's a difference
        let http_block: Box<dyn BlockHeaderable> = Box::new(dummy_block(10).unwrap().unwrap());
        let http_stream = stream::iter([http_block]);

        let ws_block_header: Box<dyn BlockHeaderable> = Box::new(block_header(0, 10).unwrap());
        let ws_stream = stream::iter([ws_block_header]);

        let mut merged_stream: Pin<Box<dyn Stream<Item = BlockNumber>>> =
            merged_stream(http_stream, ws_stream).await;

        while let Some(item) = merged_stream.next().await {
            println!("Here's the item mother fucker: {:?}", item);
        }
    }
}
