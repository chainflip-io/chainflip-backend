use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    task::Poll,
};

use futures::{future::join_all, stream, FutureExt, Stream};
use futures_lite::future::block_on;
use slog::o;
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

use crate::{eth, logging::COMPONENT_KEY, settings};

pub struct StreamAndBlocks {
    next_block_to_pop: U64,
    stream: SubscriptionStream<WebSocket, BlockHeader>,
    head_eth_stream: U64,
    web3: Web3<WebSocket>,
    interesting_past_blocks: HashMap<U64, ()>,
    logs: Vec<Log>,
}

pub async fn create_safe_eth_log_stream(
    // init_data: StreamAndBlocks,
    settings: settings::Eth,
    web3: Web3<WebSocket>,
    contract_address: H160,
    logger: &slog::Logger,
) -> impl Stream<Item = Log> {
    const BLOCKS_AWAITED: u64 = 3;

    let eth_head_stream = web3
        .clone()
        .eth_subscribe()
        .subscribe_new_heads()
        .await
        .expect("should create head stream");

    let init_data = StreamAndBlocks {
        next_block_to_pop: U64::from(0),
        stream: eth_head_stream,
        head_eth_stream: U64::from(0),
        interesting_past_blocks: Default::default(),
        web3,
        logs: Vec::default(),
    };

    let stream = Box::pin(
        stream::unfold(init_data, move |mut state| async move {
            if let Some(header) = state.stream.next().await {
                let header = header.unwrap();
                let number = header.number.unwrap();

                // check if this block has anything of interest
                let mut contract_bloom = Bloom::default();
                contract_bloom.accrue(Input::Raw(&contract_address.0));

                if header.logs_bloom.contains_bloom(&contract_bloom) {
                    println!("Yes, we have an interesting block at: {}", number);
                    state.interesting_past_blocks.insert(number, ());
                }

                state.head_eth_stream = number;
            };

            let safe_block = state
                .head_eth_stream
                .saturating_sub(U64::from(BLOCKS_AWAITED));
            if let Some(_) = state.interesting_past_blocks.get(&safe_block) {
                println!("Getting a safe block");
                let logs = state
                    .web3
                    .eth()
                    .logs(
                        FilterBuilder::default()
                            //todo: is there an "at block"
                            .from_block(BlockNumber::Number(safe_block))
                            .to_block(BlockNumber::Number(safe_block))
                            .address(vec![contract_address])
                            .build(),
                    )
                    .await
                    // have the stream return results
                    .unwrap();
                let log_stream = stream::iter(logs);
                Some((log_stream, state))
            } else {
                println!("No safe blocks to find");
                Some((stream::iter(Vec::new()), state))
            }
        })
        .flatten(),
    );
    stream
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Arc, task::Context};

    use futures::stream;
    use generic_array::typenum::U6;

    use crate::{eth, logging::utils::new_discard_logger, settings::Settings};

    use super::*;

    #[tokio::test]
    async fn gets_block_heads() {
        let settings = Settings::from_file("config/Local.toml").unwrap();
        let logger = new_discard_logger();
        let web3 = eth::new_synced_web3_client(&settings.eth, &logger)
            .await
            .expect("Failed to create Web3 WebSocket");

        let contract_address = H160::from(hex!("01BE23585060835E02B77ef475b0Cc51aA1e0709"));

        let mut stream =
            create_safe_eth_log_stream(settings.eth, web3, contract_address, &logger).await;

        while let Some(item) = stream.next().await {
            println!("Item in block: {:?}", item.block_number);
        }
    }

    // #[tokio::test]
    // async fn returns_none_when_stream_empty() {

    // }

    // #[tokio::test]
    // async fn returns_next_value_when_sufficiently_safe() {

    // }

    //..
}
