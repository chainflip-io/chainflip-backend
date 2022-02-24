use std::time::Duration;

use futures::StreamExt;
use futures::{stream, Stream};
use web3::types::U64;

use crate::constants::ETH_BLOCK_SAFETY_MARGIN;

pub const HTTP_POLL_INTERVAL: Duration = Duration::from_secs(4);

use super::{CFEthBlockHeader, EthHttpRpcApi};

use anyhow::Result;

pub async fn safe_polling_http_head_stream<EthHttpRpc: EthHttpRpcApi>(
    eth_http_rpc: EthHttpRpc,
    poll_interval: Duration,
    logger: slog::Logger,
) -> impl Stream<Item = Result<CFEthBlockHeader>> {
    struct StreamState<EthHttpRpc> {
        last_block_yielded: U64,
        last_head_fetched: U64,
        eth_http_rpc: EthHttpRpc,
        logger: slog::Logger,
    }

    let init_data = StreamState {
        last_block_yielded: U64::from(0),
        last_head_fetched: U64::from(0),
        eth_http_rpc,
        logger,
    };

    let stream = stream::unfold(init_data, move |mut state| async move {
        loop {
            let is_first_iteration =
                state.last_block_yielded == U64::from(0) && state.last_head_fetched == U64::from(0);

            // Only request the latest block number if we are out of blocks to yield
            if state.last_head_fetched
                <= state.last_block_yielded + U64::from(ETH_BLOCK_SAFETY_MARGIN)
            {
                if !is_first_iteration {
                    tokio::time::sleep(poll_interval).await;
                }
                let unsafe_block_number = state.eth_http_rpc.block_number().await.unwrap();
                assert!(unsafe_block_number.as_u64() >= ETH_BLOCK_SAFETY_MARGIN, "the fetched block number is too early in the chain to fetch a corresponding safe block");
                if unsafe_block_number + U64::from(ETH_BLOCK_SAFETY_MARGIN)
                    < state.last_head_fetched
                {
                    slog::error!(
                        &state.logger,
                        "Fetched ETH block number ({}) is more than {} blocks behind the last fetched ETH block number ({})", unsafe_block_number, ETH_BLOCK_SAFETY_MARGIN, state.last_head_fetched
                    );
                } else if unsafe_block_number < state.last_head_fetched {
                    slog::warn!(
                        &state.logger,
                        "Fetched ETH block number ({}) is less than the last fetched ETH block number ({})", unsafe_block_number, state.last_head_fetched
                    );
                }
                state.last_head_fetched = unsafe_block_number;
            }

            let next_block_to_yield = if is_first_iteration {
                state
                    .last_head_fetched
                    .checked_sub(U64::from(ETH_BLOCK_SAFETY_MARGIN))
                    .unwrap()
            } else {
                // the last block yielded was safe, so the next is +1
                state.last_block_yielded + U64::from(1)
            };
            if next_block_to_yield + U64::from(ETH_BLOCK_SAFETY_MARGIN) <= state.last_head_fetched {
                // TODO: Look at deduping with inside block_logs_stream
                let block = state
                    .eth_http_rpc
                    .block(next_block_to_yield)
                    .await
                    .and_then(|opt_block| {
                        opt_block.ok_or(anyhow::Error::msg(
                            "Could not find ETH block in HTTP safe stream",
                        ))
                    });
                state.last_block_yielded = next_block_to_yield;
                break Some((block, state));
            }
        }
    });
    let stream = stream.then(|block| async {
        block.and_then(|block| {
            if block.number.is_none() || block.logs_bloom.is_none() {
                Err(anyhow::Error::msg(
                    "HTTP block header did not contain necessary block number and/or logs bloom",
                ))
            } else {
                Ok(CFEthBlockHeader {
                    block_number: block.number.unwrap(),
                    logs_bloom: block.logs_bloom.unwrap(),
                })
            }
        })
    });

    Box::pin(stream)
}

#[cfg(test)]
pub mod tests {

    use futures::StreamExt;
    use mockall::{predicate::eq, Sequence};
    use sp_core::H256;
    use web3::types::Block;

    use super::*;

    // in tests, this can be instant
    const TEST_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(1);

    use crate::{
        eth::{mocks::MockEthHttpRpc, BlockHeaderable},
        logging::test_utils::new_test_logger,
    };

    use anyhow::Result;

    pub fn dummy_block(block_number: u64) -> Result<Option<Block<H256>>> {
        Ok(Some(Block {
            hash: Some(H256([(block_number % 256) as u8; 32])),
            number: Some(U64::from(block_number)),
            logs_bloom: Default::default(),
            ..Default::default()
        }))
    }

    #[tokio::test]
    async fn returns_best_safe_block_immediately() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        let logger = new_test_logger();

        let block_number = U64::from(10);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .returning(move || Ok(block_number));

        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .returning(move |n| dummy_block(n.as_u64()));

        let mut stream =
            safe_polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL, logger).await;
        let expected_returned_block_number = block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            expected_returned_block_number
        );
    }

    #[tokio::test]
    async fn does_not_return_block_until_progress() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let first_block_number = U64::from(10);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        // We keep getting block 10 when querying for block number
        // we only want to progress once we have a new block number
        mock_eth_http_rpc
            .expect_block_number()
            .times(10)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        // the eth chain has progressed by 1...
        let next_block_number = first_block_number + U64::from(1);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(next_block_number));

        // ...so we expect a block to be returned
        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        let mut stream =
            safe_polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL, logger).await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            expected_first_returned_block_number
        );
        let expected_next_returned_block_number =
            next_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            expected_next_returned_block_number
        );
    }

    #[tokio::test]
    async fn catches_up_if_polling_skipped_a_block_number() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let first_block_number = U64::from(10);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        // if we skip blocks, we should catch up by fetching the logs from the blocks
        // we skipped
        let num_skipped_blocks = 4;
        let next_block_number = first_block_number + U64::from(num_skipped_blocks);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(next_block_number));

        let skipped_range =
            (first_block_number.as_u64() + 1)..(first_block_number.as_u64() + num_skipped_blocks);
        for _ in skipped_range.clone() {
            mock_eth_http_rpc
                .expect_block()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move |n| dummy_block(n.as_u64()));
        }

        // first block should come in as expected
        let mut stream =
            safe_polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL, logger).await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            expected_first_returned_block_number
        );

        // we should get all the skipped blocks next (that are within the safety margin)
        for n in skipped_range {
            let expected_skipped_block_number = U64::from(n - ETH_BLOCK_SAFETY_MARGIN);
            assert_eq!(
                stream.next().await.unwrap().unwrap().block_number,
                expected_skipped_block_number
            );
        }
    }

    #[tokio::test]
    async fn if_block_number_decreases_from_last_request_wait_until_back_to_prev_latest_block() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let first_block_number = U64::from(10);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        let first_safe_block_number = first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .with(eq(first_safe_block_number))
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        let num_blocks_backwards = 2;
        let back_to_block_number = first_block_number - U64::from(num_blocks_backwards);

        // We want to return the one after the first one we have already returned
        for n in back_to_block_number.as_u64()..=first_block_number.as_u64() + 1 {
            mock_eth_http_rpc
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move || Ok(U64::from(n)));
        }

        // This is the next block that should be yielded. It shouldn't matter to the caller of .next()
        // if the chain head has decreased due to sync / reorgs
        let next_safe_block_number = first_safe_block_number + U64::from(1);
        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .with(eq(next_safe_block_number))
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        // first block should come in as expected
        let mut stream =
            safe_polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL, logger).await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            expected_first_returned_block_number
        );

        // We do not want any repeat blocks, we will just wait until we can return the next safe
        // block, after the one we've already returned
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            next_safe_block_number
        );
    }

    #[tokio::test]
    async fn if_block_numbers_increment_by_one_progresses_at_block_margin() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let block_range = 10..20;

        for block_number in block_range.clone() {
            mock_eth_http_rpc
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move || Ok(U64::from(block_number)));

            mock_eth_http_rpc
                .expect_block()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move |number| dummy_block(number.as_u64()));
        }

        let mut stream =
            safe_polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL, logger).await;
        for block_number in block_range {
            if let Some(block) = stream.next().await {
                assert_eq!(
                    block.unwrap().block_number,
                    U64::from(block_number - ETH_BLOCK_SAFETY_MARGIN)
                );
            };
        }
    }
}
