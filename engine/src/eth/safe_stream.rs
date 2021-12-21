use std::collections::{HashMap, VecDeque};

use futures::{stream, Stream};
use sp_core::H160;
use web3::{
    api::SubscriptionStream,
    transports::WebSocket,
    types::{BlockHeader, BlockNumber, FilterBuilder, Log, U64},
    Web3,
};

use ethbloom::{Bloom, Input};

use futures::StreamExt;

use crate::{eth, logging::COMPONENT_KEY, settings};

pub fn safe_eth_log_header_stream<BlockHeaderStream>(
    header_stream: BlockHeaderStream,
    safety_margin: u64,
) -> impl Stream<Item = BlockHeader>
where
    BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
{
    pub struct StreamAndBlocks<BlockHeaderStream>
    where
        BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
    {
        stream: BlockHeaderStream,
        head_eth_stream: U64,
        last_n_blocks: VecDeque<BlockHeader>,
    }
    let init_data = StreamAndBlocks {
        stream: Box::pin(header_stream),
        head_eth_stream: U64::from(0),
        last_n_blocks: Default::default(),
    };
    let stream = Box::pin(stream::unfold(init_data, move |mut state| async move {
        let loop_state = loop {
            if let Some(header) = state.stream.next().await {
                let header = header.unwrap();
                let number = header.number.unwrap();

                println!("Got block with number: {:?}", number);

                if number > state.head_eth_stream {
                    println!("We have a new block, yay.");
                    state.last_n_blocks.push_front(header);
                } else {
                    println!("reorginatoooooor");
                    let reorg_depth =
                        (state.head_eth_stream.saturating_sub(number)).saturating_add(U64::from(1));

                    // pop off the front of the queue
                    (0..reorg_depth.as_u64())
                        .map(|_| state.last_n_blocks.pop_front())
                        .for_each(drop);

                    println!("last blocks len: {:?}", state.last_n_blocks.len());
                    state.last_n_blocks.push_front(header);
                }

                println!(
                    "Update head eth stream from {:?}, to number: {:?}",
                    state.head_eth_stream, number
                );
                state.head_eth_stream = number;
            } else {
                // when the inner stream is consumed, we want to end the wrapping stream
                break None;
            }

            println!("====== DO WE YIELD? ======");
            println!(
                "back block number: {:?}",
                state.last_n_blocks.back().unwrap().number.unwrap()
            );

            println!(
                "head of stream used to calc safety: {:?}",
                state.head_eth_stream
            );

            if state
                .last_n_blocks
                .back()
                .unwrap()
                .number
                .unwrap()
                .saturating_add(U64::from(safety_margin))
                <= state.head_eth_stream
            {
                println!("Yielding block");
                break Some((state.last_n_blocks.pop_back().unwrap(), state));
            } else {
                // we don't want to return None to the caller here. Instead we want to keep progressing
                // through the inner stream
                continue;
            }
        };
        loop_state
    }));
    stream

    //         let safe_block = state
    //             .head_eth_stream
    //             .saturating_sub(U64::from(safety_margin));
    //         if let Some(_) = state.interesting_past_blocks.get(&safe_block) {
    //             println!("Getting a safe block");
    //             let logs = state
    //                 .web3
    //                 .eth()
    //                 .logs(
    //                     FilterBuilder::default()
    //                         //todo: is there an "at block"
    //                         .from_block(BlockNumber::Number(safe_block))
    //                         .to_block(BlockNumber::Number(safe_block))
    //                         .address(vec![contract_address])
    //                         .build(),
    //                 )
    //                 .await
    //                 // have the stream return results
    //                 .unwrap();
    //             let log_stream = stream::iter(logs);
    //             Some((log_stream, state))
    //         } else {
    //             println!("No safe blocks to find");
    //             Some((stream::iter(Vec::new()), state))
    //         }
    //     })
    //     .flatten(),
    // );
    // stream
}

// // check if this block has anything of interest
// let mut contract_bloom = Bloom::default();
// contract_bloom.accrue(Input::Raw(&contract_address.0));

// if header.logs_bloom.contains_bloom(&contract_bloom) {
//     println!("Yes, we have an interesting block at: {}", number);
//     state.interesting_past_blocks.insert(number, ());
// }

// async fn create_safe_eth_log_stream(
//     // init_data: StreamAndBlocks,
//     web3: Web3<WebSocket>,
//     contract_address: H160,
//     logger: &slog::Logger,
// ) -> impl Stream<Item = Log> {
//     const BLOCKS_AWAITED: u64 = 3;

//     let eth_head_stream = web3
//         .clone()
//         .eth_subscribe()
//         .subscribe_new_heads()
//         .await
//         .expect("should create head stream");

//     inner_safe_eth_log_stream(eth_head_stream, web3, contract_address, BLOCKS_AWAITED)
// }

#[cfg(test)]
mod tests {

    use crate::{eth, logging::utils::new_discard_logger, settings::Settings};

    use web3::types::H2048;

    // dev dep?
    use hex_literal::hex;
    use sp_core::H256;

    use super::*;

    const CONTRACT_ADDRESS: [u8; 20] = hex!("01BE23585060835E02B77ef475b0Cc51aA1e0709");

    fn block_header(
        hash: u8,
        block_number: u64,
        logs_bloom: H2048,
    ) -> Result<BlockHeader, web3::Error> {
        let block_header = BlockHeader {
            // fields that matter
            hash: Some(H256::from([hash; 32])),
            number: Some(U64::from(block_number)),
            logs_bloom,

            // defaults
            parent_hash: H256::default(),
            uncles_hash: H256::default(),
            author: H160::default(),
            state_root: H256::default(),
            transactions_root: H256::default(),
            receipts_root: H256::default(),
            gas_used: sp_core::U256::default(),
            gas_limit: sp_core::U256::default(),
            base_fee_per_gas: Default::default(),
            extra_data: Default::default(),
            timestamp: Default::default(),
            difficulty: sp_core::U256::default(),
            mix_hash: Default::default(),
            nonce: Default::default(),
        };
        Ok(block_header)
    }

    #[tokio::test]
    async fn returns_none_when_none_in_inner_no_safety() {
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

        let mut stream = safe_eth_log_header_stream(header_stream, 0);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_none_when_none_in_inner_with_safety() {
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

        let mut stream = safe_eth_log_header_stream(header_stream, 4);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_none_when_some_in_inner_when_safety() {
        let header_stream =
            stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![block_header(
                1,
                0,
                Default::default(),
            )]);

        let mut stream = safe_eth_log_header_stream(header_stream, 4);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_one_when_one_in_inner_but_no_more_when_no_safety() {
        let first_block = block_header(1, 0, Default::default());
        let header_stream =
            stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![first_block.clone()]);

        let mut stream = safe_eth_log_header_stream(header_stream, 0);

        assert_eq!(stream.next().await, Some(first_block.unwrap()));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_one_when_two_in_inner_but_one_safety_then_no_more() {
        let first_block = block_header(1, 0, Default::default());
        let second_block = block_header(2, 1, Default::default());
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            second_block.clone(),
            block_header(3, 2, Default::default()),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 1);

        assert_eq!(stream.next().await, Some(first_block.unwrap()));
        assert_eq!(stream.next().await, Some(second_block.unwrap()));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_duplicate_blocks_if_in_inner_when_no_safety() {
        let first_block = block_header(1, 0, Default::default());
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            first_block.clone(),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 0);

        assert_eq!(stream.next().await, Some(first_block.clone().unwrap()));
        assert_eq!(stream.next().await, Some(first_block.unwrap()));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn handles_duplicate_blocks_returned_from_api_when_safety() {
        let first_block = block_header(1, 0, Default::default());
        let second_block = block_header(2, 1, Default::default());
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            first_block.clone(),
            second_block.clone(),
            block_header(2, 2, Default::default()),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 1);

        assert_eq!(stream.next().await, Some(first_block.unwrap()));
        assert_eq!(stream.next().await, Some(second_block.unwrap()));
        assert!(stream.next().await.is_none());
    }

    // #[tokio::test]
    // async fn returns_none_when_one_in_inner_before_safety_margin_reached() {
    //     let settings = Settings::from_file("config/Local.toml").unwrap();
    //     let logger = new_discard_logger();

    //     let web3 = eth::new_synced_web3_client(&settings.eth, &logger)
    //         .await
    //         .expect("Failed to create Web3 WebSocket");

    //     let contract_address = H160::from(hex!("01BE23585060835E02B77ef475b0Cc51aA1e0709"));

    //     let first_block = block_header(1, 0, Default::default());
    //     let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
    //         first_block,
    //         block_header(2, 0, Default::default()),
    //     ]);

    //     let mut stream = inner_safe_eth_log_stream(header_stream, web3, contract_address, 2);

    //     // assert_eq!(stream.next().await, Some(Ok(first_block)));
    // }

    // #[tokio::test]
    // async fn returns_none_when_stream_empty() {

    // }

    // #[tokio::test]
    // async fn returns_next_value_when_sufficiently_safe() {

    // }

    //..
}
