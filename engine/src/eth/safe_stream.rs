use std::{ops::Range, task::Poll};

use futures::Stream;
use sp_core::H160;
use web3::{
    api::SubscriptionStream,
    transports::WebSocket,
    types::{BlockHeader, BlockNumber, FilterBuilder, Log, U64},
    Web3,
};

/// Contains a blocks worth of logs for a particular contract
type LogBlock = Vec<Log>;

use futures::StreamExt;

/// Subscribe safely to future ETH blocks
pub struct SafeEthBlockStream {
    // The head of the eth subscription to the node
    current_eth_block_head: U64,
    // the eth log subscription stream
    eth_head_stream: SubscriptionStream<WebSocket, BlockHeader>,

    current_block_logs: LogBlock,
    // the block in the last_five_blocks array we are currently pointing to
    last_n_index: std::iter::Cycle<Range<u64>>,
    last_n_blocks: Vec<LogBlock>,

    blocks_head_is_ahead: u64,

    web3: Web3<WebSocket>,
}

impl SafeEthBlockStream {
    pub async fn new(web3: &Web3<WebSocket>, block_safety: u64) -> Self {
        let eth_head_stream = web3
            .eth_subscribe()
            .subscribe_new_heads()
            .await
            .expect("should create head stream");

        Self {
            eth_head_stream,
            current_eth_block_head: U64::default(),
            current_block_logs: Default::default(),
            last_n_index: (0..block_safety).cycle(),
            last_n_blocks: Default::default(),
            blocks_head_is_ahead: block_safety,
            web3: web3.clone(),
        }
    }
}

impl SafeEthBlockStream {
    async fn get_next_head(&mut self) {
        while let Some(stuff) = self.eth_head_stream.next().await {
            println!("Here's some stuff: {:?}", stuff);
        }
    }
}

impl Stream for SafeEthBlockStream {
    type Item = Log;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // The number of ETH blocks we wait until we decide we are safe enough
        // to avoid returning duplicate events due to reorgs
        const BLOCKS_AWAITED: u64 = 5;

        use futures_lite::stream::StreamExt;
        while let Poll::Ready(Some(Ok(log))) = self.eth_head_stream.poll_next(cx) {
            let log_block_number = log.block_number.unwrap();
            if log_block_number < self.current_eth_block_head {
                // we have reorgd.
                // we now want to reset the blocks from there.
                // e.g. if we had blocks [10, 11, 12, 13, 14] in our Vec,
                // and we received a log with block number 12, we want to reset the Vec indexes
                // storing blocks 12, 13, 14

                let num_reorged = self
                    .current_eth_block_head
                    .saturating_sub(log.block_number.unwrap());

                // because we've effectively reset the count, we now have fewer than BLOCKS_AWAITED actually
                // awaited, because we have to go back to those blocks
                let to_skip = BLOCKS_AWAITED.saturating_sub(num_reorged.as_u64());
                self.blocks_head_is_ahead = BLOCKS_AWAITED.saturating_sub(to_skip);
                // self.blocks_head_is_ahead =
                // get to the block that we want to remove
                // advance_by is an experimental api that makes this nicer
                for _ in 0..to_skip {
                    self.last_n_index.next().unwrap();
                }
                // we want to reset and then add this event, to start again
                self.current_block_logs = Vec::new();
                self.current_block_logs.push(log.clone());
            } else if log_block_number > self.current_eth_block_head {
                // we have progressed through the head of the stream and we have not reorged
                // add events to our backlog

                // can probs remove clone here
                // let mut peekable_index = self.last_n_index.clone().peekable();
                // let index = peekable_index.peek().unwrap();

                let index = self.last_n_index.next().unwrap();
                let current_block_logs = self.current_block_logs.clone();
                self.last_n_blocks
                    .insert(index as usize, current_block_logs);

                self.current_block_logs = Vec::new();

                // we are catching up, zoom zoom
                if self.blocks_head_is_ahead > 0 {
                    self.blocks_head_is_ahead = self.blocks_head_is_ahead.saturating_sub(1);
                }

                // we want to add all the events in a single block
            } else if log_block_number == self.current_eth_block_head {
                // keep appending to this current blocks worth of blocks
                self.current_block_logs.push(log.clone());
            }

            self.current_eth_block_head = log_block_number;
        }

        // if we are now 5 blocks ahead of the backlog we have stored, we can be pretty sure
        // we won't reorg from here. Otherwise, continue along until we do
        if self.blocks_head_is_ahead == 0 {
            return Poll::Ready(self.last_n_blocks.clone().into_iter().flatten().next());
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use crate::{eth, logging::utils::new_discard_logger, settings::Settings};

    use super::*;

    #[tokio::test]
    async fn gets_block_heads() {
        let settings = Settings::from_file("config/Local.toml").unwrap();
        let logger = new_discard_logger();

        let web3 = eth::new_synced_web3_client(&settings.eth, &logger)
            .await
            .expect("Failed to create Web3 WebSocket");

        let safe_stream = SafeEthBlockStream::new(&web3, 6);
    }

    // #[tokio::test]
    // async fn returns_none_when_stream_empty() {

    // }

    // #[tokio::test]
    // async fn returns_next_value_when_sufficiently_safe() {

    // }

    //..
}
