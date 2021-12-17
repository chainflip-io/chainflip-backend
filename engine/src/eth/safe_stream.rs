use std::{collections::HashMap, ops::Range, task::Poll};

use futures::{FutureExt, Stream};
use sp_core::H160;
use web3::{
    api::SubscriptionStream,
    transports::WebSocket,
    types::{BlockHeader, BlockNumber, FilterBuilder, Log, U64},
    Web3,
};

use ethbloom::{Bloom, Input};

use hex_literal::hex;

/// Contains a blocks worth of logs for a particular contract
type LogBlock = Vec<Log>;

use futures::StreamExt;

/// Subscribe safely to future ETH blocks
pub struct SafeEthBlockStream {
    // The head of the eth subscription to the node
    current_eth_block_head: U64,
    // the eth log subscription stream
    eth_head_stream: SubscriptionStream<WebSocket, BlockHeader>,

    contract_address: H160,

    // the block in the last_five_blocks array we are currently pointing to
    // last_n_index: std::iter::Cycle<Range<u64>>,
    blocks_head_is_ahead: u64,

    interesting_past_blocks: HashMap<U64, ()>,

    safe_block_number: U64,

    web3: Web3<WebSocket>,
}

impl SafeEthBlockStream {
    pub async fn new(web3: &Web3<WebSocket>, contract_address: H160, block_safety: u64) -> Self {
        let eth_head_stream = web3
            .eth_subscribe()
            .subscribe_new_heads()
            .await
            .expect("should create head stream");

        Self {
            eth_head_stream,
            current_eth_block_head: U64::default(),
            // last_n_index: (0..block_safety).cycle(),
            // last_n_blocks: Default::default(),
            contract_address,
            interesting_past_blocks: Default::default(),
            blocks_head_is_ahead: block_safety,
            safe_block_number: U64::from(0),
            web3: web3.clone(),
        }
    }
}

impl SafeEthBlockStream {
    async fn get_next_head(&mut self) {
        while let Some(header) = self.eth_head_stream.next().await {
            let header = header.unwrap();
            println!("Got head for block number: {:?}", header.number);

            let logs_bloom = header.logs_bloom;
            let transfer_chainlink_topic =
                hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
            let chainlink_rinkeby_address = hex!("01be23585060835e02b77ef475b0cc51aa1e0709");

            let mut my_bloom = Bloom::default();
            my_bloom.accrue(Input::Raw(&chainlink_rinkeby_address));
            my_bloom.accrue(Input::Raw(&transfer_chainlink_topic));

            let contains_bloom = logs_bloom.contains_bloom(&my_bloom);

            println!(
                "Does this block contain a chainlink transfer? {:?}",
                contains_bloom
            );
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
        while let Poll::Ready(Some(Ok(header))) = self.eth_head_stream.poll_next(cx) {
            let header_block_number = header.number.unwrap();

            // find the logs that are relevant to our contract only

            // let address = hex!("ef2d6d194084c2de36e0dabfce45d046b37d1106");
            // let topic = hex!("02c69be41d0b7e40352fc85be1cd65eb03d40ef8427a0ca4596b1ead9a00e9fc");
            // let mut my_bloom = Bloom::default();
            // assert!(!my_bloom.contains_input(Input::Raw(&address)));
            // assert!(!my_bloom.contains_input(Input::Raw(&topic)));

            // we need to check if we have *might* have logs we care about in this log bloom
            if header_block_number < self.current_eth_block_head {
                // we have reorgd.
                // we now want to reset the blocks from there.
                // e.g. if we had blocks [10, 11, 12, 13, 14] in our Vec,
                // and we received a log with block number 12, we want to reset the Vec indexes
                // storing blocks 12, 13, 14

                let num_reorged = self
                    .current_eth_block_head
                    .saturating_sub(header_block_number);
            } else if header_block_number > self.current_eth_block_head {
                let mut contract_bloom = Bloom::default();
                contract_bloom.accrue(Input::Raw(&self.contract_address.0));

                let contains_bloom = header.logs_bloom.contains_bloom(&contract_bloom);
                println!(
                    "Does this block contain a chainlink transfer? {:?}",
                    contains_bloom
                );

                self.interesting_past_blocks.insert(header_block_number, ());

                // we have progressed through the head of the stream and we have not reorged
                // add events to our backlog

                // we want to add all the events in a single block
            } else if header_block_number == self.current_eth_block_head {
                // keep appending to this current blocks worth of blocks
                // self.current_block_logs.push(log.clone());
            }

            self.current_eth_block_head = header_block_number;
        }

        // if we are now 5 blocks ahead of the backlog we have stored, we can be pretty sure
        // we won't reorg from here. Otherwise, continue along until we do
        // is the block 5 behind interesting
        let safe_block = self
            .current_eth_block_head
            .saturating_sub(U64::from(BLOCKS_AWAITED));
        if let Some(_) = self.interesting_past_blocks.get(&safe_block) {
            // get the block
            while let Poll::Ready(Ok(logs)) = self
                .web3
                .eth()
                .logs(
                    FilterBuilder::default()
                        .from_block(BlockNumber::Number(safe_block))
                        .to_block(BlockNumber::Number(safe_block))
                        .address(vec![self.contract_address])
                        .build(),
                )
                .poll_unpin(cx)
            {
                println!("HELLOOOOOO, we have a block for you sir");
                println!("LOGS: {:?}", logs);

                // I don't think this is correct. We have to "return" but also, stay here for the next call to poll.
                // maybe we have to do that via control flow
                for log in logs {
                    return Poll::Ready(Some(log));
                }
            }
        } else {
            println!("Nothing here mate haha.")
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

        let mut safe_stream = SafeEthBlockStream::new(&web3, 6).await;

        safe_stream.get_next_head().await;
    }

    // #[tokio::test]
    // async fn returns_none_when_stream_empty() {

    // }

    // #[tokio::test]
    // async fn returns_next_value_when_sufficiently_safe() {

    // }

    //..
}
