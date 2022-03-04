use std::collections::VecDeque;

use futures::{stream, Stream};
use web3::types::{BlockHeader, U64};

use futures::StreamExt;

use super::EthNumberBloom;

use anyhow::Result;

pub fn safe_ws_head_stream<BlockHeaderStream>(
    header_stream: BlockHeaderStream,
    safety_margin: u64,
) -> impl Stream<Item = Result<EthNumberBloom>>
where
    BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
{
    struct StreamAndBlocks<BlockHeaderStream>
    where
        BlockHeaderStream: Stream<Item = Result<BlockHeader, web3::Error>>,
    {
        stream: BlockHeaderStream,
        last_block_yielded: U64,
        unsafe_block_headers: VecDeque<EthNumberBloom>,
    }
    let init_data = StreamAndBlocks {
        stream: Box::pin(header_stream),
        last_block_yielded: U64::from(0),
        unsafe_block_headers: Default::default(),
    };

    Box::pin(stream::unfold(init_data, move |mut state| async move {
        loop {
            if let Some(header) = state.stream.next().await {
                // NB: Here we are returning the head error, as if it were a block error
                // so if the head is an error, at block 14, and the safety margin is 4
                // it looks like the error was at block 10
                let current_header = match header {
                    Ok(header) => header,
                    Err(e) => break Some((Err(e.into()), state)),
                };
                let current_block_number = match current_header.number.ok_or_else(|| {
                    anyhow::Error::msg("Latest WS block header does not have a block number.")
                }) {
                    Ok(number) => number,
                    Err(err) => break Some((Err(err), state)),
                };

                // Terminate stream if we have skipped into the future
                let is_first_iteration = state.last_block_yielded == U64::from(0);
                if !is_first_iteration && current_block_number > state.last_block_yielded + 1 {
                    break None;
                }

                if let Some(last_unsafe_block_header) = state.unsafe_block_headers.back() {
                    let last_unsafe_block_number = last_unsafe_block_header.block_number;

                    assert!(current_block_number <= last_unsafe_block_number + 1);
                    if current_block_number <= last_unsafe_block_number {
                        // if we receive two of the same block number then we still need to drop the first
                        let reorg_depth =
                            (last_unsafe_block_number - current_block_number) + U64::from(1);

                        (0..reorg_depth.as_u64()).for_each(|_| {
                            state.unsafe_block_headers.pop_back();
                        });
                    }
                }

                state.unsafe_block_headers.push_back(EthNumberBloom {
                    block_number: current_block_number,
                    logs_bloom: current_header.logs_bloom,
                });

                if let Some(header) = state.unsafe_block_headers.front() {
                    if header.block_number.saturating_add(U64::from(safety_margin))
                        <= current_block_number
                    {
                        state.last_block_yielded = header.block_number;
                        break Some((
                            Ok(state
                                .unsafe_block_headers
                                .pop_front()
                                .expect("already checked for item above")),
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

#[cfg(test)]
pub mod tests {

    use sp_core::{H160, H256};

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

    impl From<BlockHeader> for EthNumberBloom {
        fn from(block_header: BlockHeader) -> Self {
            EthNumberBloom {
                block_number: block_header.number.unwrap(),
                logs_bloom: block_header.logs_bloom,
            }
        }
    }

    #[tokio::test]
    async fn returns_none_when_none_in_inner_no_safety() {
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

        let mut stream = safe_ws_head_stream(header_stream, 0);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_none_when_none_in_inner_with_safety() {
        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![]);

        let mut stream = safe_ws_head_stream(header_stream, 4);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_none_when_some_in_inner_when_safety() {
        let header_stream =
            stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![block_header(1, 0)]);

        let mut stream = safe_ws_head_stream(header_stream, 4);

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn returns_one_when_one_in_inner_but_no_more_when_no_safety() {
        let first_block = block_header(1, 0);
        let header_stream =
            stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![first_block.clone()]);

        let mut stream = safe_ws_head_stream(header_stream, 0);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block.unwrap().into()
        );
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

        let mut stream = safe_ws_head_stream(header_stream, 1);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block.unwrap().into()
        );
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            second_block.unwrap().into()
        );
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

        let mut stream = safe_ws_head_stream(header_stream, 0);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block.clone().unwrap().into()
        );
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block_prime.unwrap().into()
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

        let mut stream = safe_ws_head_stream(header_stream, 1);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block_prime.unwrap().into()
        );
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            second_block_prime.unwrap().into()
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

        let mut stream = safe_ws_head_stream(header_stream, 2);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block_prime.unwrap().into()
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn safe_stream_terminates_when_input_stream_skips_into_future() {
        let first_block = block_header(1, 11);

        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            block_header(2, 12),
            block_header(7, 17),
        ]);

        let mut stream = safe_ws_head_stream(header_stream, 1);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block.unwrap().into()
        );

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn safe_stream_passes_through_error_header_no_safety() {
        // just any web3 error
        let first_block = block_header(1, 11);
        let error_block = Err(web3::Error::Internal);

        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            error_block.clone(),
        ]);

        let mut stream = safe_ws_head_stream(header_stream, 0);

        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            first_block.unwrap().into()
        );

        assert!(stream.next().await.unwrap().is_err());
    }

    #[tokio::test]
    async fn safe_stream_does_not_return_prematurely_on_error_header_with_safety() {
        let first_block = block_header(1, 11);
        let error_block = Err(web3::Error::Internal);

        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            error_block.clone(),
        ]);

        let mut stream = safe_ws_head_stream(header_stream, 1);

        assert!(stream.next().await.unwrap().is_err());
    }

    // Ensure we return the errors when we don't have a block number,
    // in the same way as the error of pulling the header from the ws stream itself
    #[tokio::test]
    async fn safe_stream_returns_error_within_header_with_safety() {
        let first_block = block_header(1, 11);
        let mut second_block = block_header(1, 11).unwrap();
        second_block.number = None;

        let header_stream = stream::iter::<Vec<Result<BlockHeader, web3::Error>>>(vec![
            first_block.clone(),
            Ok(second_block.clone()),
        ]);

        let mut stream = safe_ws_head_stream(header_stream, 1);

        assert!(stream.next().await.unwrap().is_err());
    }
}
