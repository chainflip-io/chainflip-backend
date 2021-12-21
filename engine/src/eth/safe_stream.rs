use std::collections::HashMap;

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

pub fn inner_safe_eth_log_stream<BlockHeaderStream>(
    header_stream: BlockHeaderStream,
    web3: Web3<WebSocket>,
    contract_address: H160,
    safety_margin: u64,
) -> impl Stream<Item = Log>
where
    BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
{
    pub struct StreamAndBlocks<BlockHeaderStream>
    where
        BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
    {
        stream: BlockHeaderStream,
        head_eth_stream: U64,
        web3: Web3<WebSocket>,
        interesting_past_blocks: HashMap<U64, ()>,
    }
    let init_data = StreamAndBlocks {
        stream: Box::pin(header_stream),
        head_eth_stream: U64::from(0),
        interesting_past_blocks: Default::default(),
        web3,
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
                .saturating_sub(U64::from(safety_margin));
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

async fn create_safe_eth_log_stream(
    // init_data: StreamAndBlocks,
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

    inner_safe_eth_log_stream(eth_head_stream, web3, contract_address, BLOCKS_AWAITED)
}

#[cfg(test)]
mod tests {

    use crate::{eth, logging::utils::new_discard_logger, settings::Settings};

    use web3::types::H2048;

    // dev dep?
    use hex_literal::hex;
    use sp_core::H256;

    use super::*;

    fn block_header(hash: u8, block_number: u64, bloom: H2048) {
        let block_header = BlockHeader {
            // fields that matter
            hash: Some(H256::from([hash; 32])),
            number: Some(U64::from(block_number)),
            logs_bloom: H2048::default(),

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
    }

    #[tokio::test]
    async fn gets_block_heads() {
        let settings = Settings::from_file("config/Local.toml").unwrap();
        let logger = new_discard_logger();

        let web3 = eth::new_synced_web3_client(&settings.eth, &logger)
            .await
            .expect("Failed to create Web3 WebSocket");

        let contract_address = H160::from(hex!("01BE23585060835E02B77ef475b0Cc51aA1e0709"));

        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

        let mut stream = inner_safe_eth_log_stream(header_stream, web3, contract_address, 2);

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
