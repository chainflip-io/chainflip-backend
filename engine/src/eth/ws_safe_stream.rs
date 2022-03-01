use std::collections::VecDeque;

use futures::{stream, Stream};
use sp_core::H160;
use web3::types::{BlockHeader, BlockNumber, FilterBuilder, Log, U64};

use ethbloom::{Bloom, Input};

use futures::StreamExt;

use crate::eth::BlockHeaderable;

use super::EthRpcApi;

use anyhow::Result;

pub fn safe_ws_head_stream<BlockHeaderStream>(
    header_stream: BlockHeaderStream,
    safety_margin: u64,
) -> impl Stream<Item = Result<BlockHeader>>
where
    BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
{
    struct StreamAndBlocks<BlockHeaderStream>
    where
        BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
    {
        stream: BlockHeaderStream,
        unsafe_block_headers: VecDeque<BlockHeader>,
    }
    let init_data = StreamAndBlocks {
        stream: Box::pin(header_stream),
        unsafe_block_headers: Default::default(),
    };

    Box::pin(stream::unfold(init_data, move |mut state| async move {
        loop {
            if let Some(header) = state.stream.next().await {
                let header = match header {
                    Ok(header) => header,
                    Err(e) => break Some((Err(e.into()), state)),
                };
                let number = header.number.unwrap();

                if let Some(last_unsafe_block_header) = state.unsafe_block_headers.back() {
                    let last_unsafe_block_number = last_unsafe_block_header.number().unwrap();
                    assert!(number <= last_unsafe_block_number + 1);
                    if number <= last_unsafe_block_number {
                        // if we receive two of the same block number then we still need to drop the first
                        let reorg_depth = (last_unsafe_block_number - number) + U64::from(1);

                        (0..reorg_depth.as_u64()).for_each(|_| {
                            state.unsafe_block_headers.pop_back();
                        });
                    }
                }

                state.unsafe_block_headers.push_back(header);

                if let Some(header) = state.unsafe_block_headers.front() {
                    if header
                        .number
                        .expect("all blocks on the chain have block numbers")
                        .saturating_add(U64::from(safety_margin))
                        <= number
                    {
                        break Some((
                            Ok(state
                                .unsafe_block_headers
                                .pop_front()
                                .expect("already put an item above")),
                            state,
                        ));
                    } else {
                        // we don't want to return None to the caller here. Instead we want to keep progressing
                        // through the inner stream
                        continue;
                    }
                }
            } else {
                // when the inner stream is consumed, we want to end the wrapping/safe stream
                break None;
            }
        }
    }))
}

pub async fn filtered_log_stream_by_contract<SafeBlockHeaderStream, EthRpc, EthBlockHeader>(
    safe_eth_head_stream: SafeBlockHeaderStream,
    eth_rpc: EthRpc,
    contract_address: H160,
    logger: slog::Logger,
) -> impl Stream<Item = Log>
where
    SafeBlockHeaderStream: Stream<Item = EthBlockHeader>,
    EthRpc: EthRpcApi + Clone,
    EthBlockHeader: BlockHeaderable + Clone,
{
    let my_stream = safe_eth_head_stream
        .filter_map(move |header| {
            let block_number = header.number().unwrap();
            slog::debug!(logger, "Observing ETH block: `{}`", block_number);
            let eth_rpc = eth_rpc.clone();
            let logger = logger.clone();
            async move {
                let mut contract_bloom = Bloom::default();
                contract_bloom.accrue(Input::Raw(&contract_address.0));

                if header
                    .clone()
                    .logs_bloom()
                    .unwrap()
                    .contains_bloom(&contract_bloom)
                {
                    match eth_rpc
                        .get_logs(
                            FilterBuilder::default()
                                .from_block(BlockNumber::Number(block_number))
                                .to_block(BlockNumber::Number(block_number))
                                .address(vec![contract_address])
                                .build(),
                        )
                        .await
                    {
                        Ok(logs) => Some(stream::iter(logs)),
                        Err(err) => {
                            slog::error!(
                                logger,
                                "Failed to request ETH logs for block `{}`: {}",
                                block_number,
                                err,
                            );
                            None
                        }
                    }
                } else {
                    None
                }
            }
        })
        .flatten();

    Box::pin(my_stream)
}

#[cfg(test)]
pub mod tests {

    use sp_core::H256;

    use super::*;

    pub fn block_header(hash: u8, block_number: u64) -> Result<BlockHeader, web3::Error> {
        let block_header = BlockHeader {
            // fields that matter
            hash: Some(H256::from([hash; 32])),
            number: Some(U64::from(block_number)),

            // defaults
            logs_bloom: Default::default(),
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
            stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![block_header(1, 0)]);

        let mut stream = safe_eth_log_header_stream(header_stream, 4);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_one_when_one_in_inner_but_no_more_when_no_safety() {
        let first_block = block_header(1, 0);
        let header_stream =
            stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![first_block.clone()]);

        let mut stream = safe_eth_log_header_stream(header_stream, 0);

        assert_eq!(stream.next().await.unwrap().unwrap(), first_block.unwrap());
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_one_when_two_in_inner_but_one_safety_then_no_more() {
        let first_block = block_header(1, 0);
        let second_block = block_header(2, 1);
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            second_block.clone(),
            block_header(3, 2),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 1);

        assert_eq!(stream.next().await.unwrap().unwrap(), first_block.unwrap());
        assert_eq!(stream.next().await.unwrap().unwrap(), second_block.unwrap());
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_reorgs_of_depth_1_blocks_if_in_inner_when_no_safety() {
        // NB: Same block number, different blocks. Our node saw two blocks at the same height, so returns them both
        let first_block = block_header(1, 0);
        let first_block_prime = block_header(2, 0);
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            first_block_prime.clone(),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 0);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block.clone().unwrap()
        );
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block_prime.unwrap()
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn handles_reogs_depth_1_blocks_when_safety() {
        let first_block = block_header(1, 0);
        let first_block_prime = block_header(11, 0);
        let second_block_prime = block_header(2, 1);
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            first_block_prime.clone(),
            second_block_prime.clone(),
            block_header(2, 2),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 1);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block_prime.unwrap()
        );
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            second_block_prime.unwrap()
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn safe_stream_when_reorg_of_depth_below_safety() {
        let first_block = block_header(1, 10);
        let second_block = block_header(2, 11);
        let first_block_prime = block_header(11, 10);
        let second_block_prime = block_header(21, 11);
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            second_block.clone(),
            first_block_prime.clone(),
            second_block_prime.clone(),
            block_header(2, 12),
        ]);

        let mut stream = safe_eth_log_header_stream(header_stream, 2);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block_prime.unwrap()
        );
        assert!(stream.next().await.is_none());
    }
}
