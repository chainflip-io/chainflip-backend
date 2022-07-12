use std::time::Duration;

use futures::{stream, Stream};
use slog::o;
use tokio::time::Interval;
use web3::types::U64;

use crate::{common::make_periodic_tick, logging::COMPONENT_KEY};

pub const HTTP_POLL_INTERVAL: Duration = Duration::from_secs(4);

use super::{EthNumberBloom, EthRpcApi};

use futures::StreamExt;

pub async fn safe_polling_http_head_stream<HttpRpc>(
    eth_http_rpc: HttpRpc,
    poll_interval: Duration,
    safety_margin: u64,
    logger: &slog::Logger,
) -> impl Stream<Item = EthNumberBloom>
where
    HttpRpc: EthRpcApi,
{
    struct StreamState<HttpRpc> {
        option_last_block_yielded: Option<U64>,
        option_last_head_fetched: Option<U64>,
        eth_http_rpc: HttpRpc,
        poll_interval: Interval,
        logger: slog::Logger,
    }

    let init_state = StreamState {
        option_last_block_yielded: None,
        option_last_head_fetched: None,
        eth_http_rpc,
        poll_interval: make_periodic_tick(poll_interval, false),
        logger: logger.new(o!(COMPONENT_KEY => "ETH_HTTPSafeStream")),
    };

    Box::pin(
        stream::unfold(init_state, move |mut state| async move {
            let StreamState {
                option_last_block_yielded,
                option_last_head_fetched,
                eth_http_rpc,
                poll_interval,
                logger,
            } = &mut state;

            async fn get_unsafe_block_number<HttpRpc: EthRpcApi>(
                eth_http_rpc: &HttpRpc,
                poll_interval: &mut Interval,
                logger: &slog::Logger,
            ) -> U64 {
                loop {
                    match eth_http_rpc.block_number().await {
                        Ok(block_number) => break block_number,
                        Err(err) => {
                            slog::error!(logger, "Error fetching ETH block number: {}", err);
                            poll_interval.tick().await;
                        }
                    };
                }
            }

            let last_head_fetched = if let Some(last_head_fetched) = option_last_head_fetched {
                last_head_fetched
            } else {
                option_last_head_fetched
                    .insert(get_unsafe_block_number(eth_http_rpc, poll_interval, logger).await)
            };

            // Only request the latest block number if we are out of blocks to yield
            while {
                if let Some(last_block_yielded) = option_last_block_yielded {
                    *last_head_fetched <= *last_block_yielded + U64::from(safety_margin)
                } else {
                    *last_head_fetched < U64::from(safety_margin)
                }
            } {
                poll_interval.tick().await;
                let unsafe_block_number =
                    get_unsafe_block_number(eth_http_rpc, poll_interval, logger).await;

                // Fetched unsafe_block_number is more than `safety_margin` blocks behind the last fetched ETH block number (last_head_fetched)
                if unsafe_block_number + safety_margin < *last_head_fetched {
                    return None;
                }

                *last_head_fetched = unsafe_block_number;
            }

            let next_block_to_yield = if let Some(last_block_yielded) = option_last_block_yielded {
                // the last block yielded was safe, so the next is +1
                *last_block_yielded + 1
            } else {
                last_head_fetched
                    .checked_sub(U64::from(safety_margin))
                    .unwrap()
            };

            let block = loop {
                match eth_http_rpc.block(next_block_to_yield).await {
                    Ok(block) => {
                        break block;
                    }
                    Err(err) => {
                        slog::error!(logger, "Error fetching ETH block: {}", err);
                        poll_interval.tick().await;
                    }
                }
            };
            let number_bloom = EthNumberBloom::try_from(block).ok()?;
            *option_last_block_yielded = Some(number_bloom.block_number);
            Some((number_bloom, state))
        })
        .fuse(),
    )
}

#[cfg(test)]
pub mod tests {

    use futures::StreamExt;
    use mockall::{predicate::eq, Sequence};
    use sp_core::H256;
    use sp_core::U256;
    use web3::types::Block;
    use web3::types::H2048;

    use super::*;

    // in tests, this can be instant
    const TEST_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(1);

    use crate::logging::test_utils::new_test_logger;
    use crate::{constants::ETH_BLOCK_SAFETY_MARGIN, eth::rpc::mocks::MockEthHttpRpcClient};

    use anyhow::Result;

    pub fn dummy_block(block_number: u64) -> Result<Block<H256>> {
        Ok(Block {
            hash: Some(H256([(block_number % 256) as u8; 32])),
            number: Some(U64::from(block_number)),
            logs_bloom: Some(H2048::default()),
            base_fee_per_gas: Some(U256::from(1)),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn returns_best_safe_block_immediately() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let block_number = U64::from(10);
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .returning(move || Ok(block_number));

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .returning(move |n| dummy_block(n.as_u64()));

        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;
        let expected_returned_block_number = block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().block_number,
            expected_returned_block_number
        );
    }

    #[tokio::test]
    async fn does_not_return_until_chain_head_is_beyond_safety_margin() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let range = 1..=ETH_BLOCK_SAFETY_MARGIN;
        for n in range {
            mock_eth_http_rpc_client
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move || Ok(U64::from(n)));
        }

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| {
                println!("{}", n);
                dummy_block(n.as_u64())
            });

        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;
        assert_eq!(stream.next().await.unwrap().block_number, U64::from(0));
    }

    #[tokio::test]
    async fn does_not_return_block_until_progress() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let first_block_number = U64::from(10);
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        // We keep getting block 10 when querying for block number
        // we only want to progress once we have a new block number
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(10)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        // the eth chain has progressed by 1...
        let next_block_number = first_block_number + U64::from(1);
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(next_block_number));

        // ...so we expect a block to be returned
        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().block_number,
            expected_first_returned_block_number
        );
        let expected_next_returned_block_number =
            next_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().block_number,
            expected_next_returned_block_number
        );
    }

    #[tokio::test]
    async fn catches_up_if_polling_skipped_a_block_number() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let first_block_number = U64::from(10);
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        // if we skip blocks, we should catch up by fetching the logs from the blocks
        // we skipped
        let num_skipped_blocks = 4;
        let next_block_number = first_block_number + U64::from(num_skipped_blocks);
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(next_block_number));

        let skipped_range =
            (first_block_number.as_u64() + 1)..(first_block_number.as_u64() + num_skipped_blocks);
        for _ in skipped_range.clone() {
            mock_eth_http_rpc_client
                .expect_block()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move |n| dummy_block(n.as_u64()));
        }

        // first block should come in as expected
        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().block_number,
            expected_first_returned_block_number
        );

        // we should get all the skipped blocks next (that are within the safety margin)
        for n in skipped_range {
            let expected_skipped_block_number = U64::from(n - ETH_BLOCK_SAFETY_MARGIN);
            assert_eq!(
                stream.next().await.unwrap().block_number,
                expected_skipped_block_number
            );
        }
    }

    #[tokio::test]
    async fn if_block_number_decreases_from_last_request_wait_until_back_to_prev_latest_block() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let first_block_number = U64::from(10);
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(first_block_number));

        let first_safe_block_number = first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .with(eq(first_safe_block_number))
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        let num_blocks_backwards = 2;
        let back_to_block_number = first_block_number - U64::from(num_blocks_backwards);

        // We want to return the one after the first one we have already returned
        for n in back_to_block_number.as_u64()..=first_block_number.as_u64() + 1 {
            mock_eth_http_rpc_client
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move || Ok(U64::from(n)));
        }

        // This is the next block that should be yielded. It shouldn't matter to the caller of .next()
        // if the chain head has decreased due to sync / reorgs
        let next_safe_block_number = first_safe_block_number + U64::from(1);
        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .with(eq(next_safe_block_number))
            .in_sequence(&mut seq)
            .returning(move |n| dummy_block(n.as_u64()));

        // first block should come in as expected
        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().block_number,
            expected_first_returned_block_number
        );

        // We do not want any repeat blocks, we will just wait until we can return the next safe
        // block, after the one we've already returned
        assert_eq!(
            stream.next().await.unwrap().block_number,
            next_safe_block_number
        );
    }

    #[tokio::test]
    async fn if_block_numbers_increment_by_one_progresses_at_block_margin() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let block_range = 10..20;

        for block_number in block_range.clone() {
            mock_eth_http_rpc_client
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move || Ok(U64::from(block_number)));

            mock_eth_http_rpc_client
                .expect_block()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move |number| dummy_block(number.as_u64()));
        }

        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;
        for block_number in block_range {
            if let Some(block) = stream.next().await {
                assert_eq!(
                    block.block_number,
                    U64::from(block_number - ETH_BLOCK_SAFETY_MARGIN)
                );
            };
        }
    }

    #[tokio::test]
    async fn stalls_on_bad_block_number_poll() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let block_range = 10..=12;

        for block_number in block_range.clone() {
            mock_eth_http_rpc_client
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move || Ok(U64::from(block_number)));

            mock_eth_http_rpc_client
                .expect_block()
                .times(1)
                .in_sequence(&mut seq)
                .returning(move |number| dummy_block(number.as_u64()));
        }

        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Err(anyhow::Error::msg("Failed to get block number, you fool")));

        let block_number_after_error = 13;
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(U64::from(block_number_after_error)));

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |number| dummy_block(number.as_u64()));

        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            &logger,
        )
        .await;

        for block_number in block_range {
            if let Some(block) = stream.next().await {
                assert_eq!(
                    block.block_number,
                    U64::from(block_number - ETH_BLOCK_SAFETY_MARGIN)
                );
            };
        }

        assert_eq!(
            stream.next().await.unwrap().block_number,
            U64::from(block_number_after_error - ETH_BLOCK_SAFETY_MARGIN)
        );
    }

    #[tokio::test]
    async fn stall_when_failed_to_fetch_safe_block() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let logger = new_test_logger();

        let mut seq = Sequence::new();

        let safety_margin = 2;

        // === success ===
        let first_block = 10;
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(U64::from(first_block)));

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |number| dummy_block(number.as_u64()));

        // === successfully fetch block number, but fail getting block ===
        let second_block = first_block + 1;
        mock_eth_http_rpc_client
            .expect_block_number()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move || Ok(U64::from(second_block)));

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |_number| Err(anyhow::Error::msg("Fetch block failed :(")));

        // === second block success ===
        // We don't refetch the block number here. We don't need to, since we still need to yield block 11

        mock_eth_http_rpc_client
            .expect_block()
            .times(1)
            .in_sequence(&mut seq)
            .returning(move |number| dummy_block(number.as_u64()));

        // === ===
        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            safety_margin,
            &logger,
        )
        .await;

        assert_eq!(
            stream.next().await.unwrap().block_number,
            U64::from(first_block - safety_margin)
        );

        assert_eq!(
            stream.next().await.unwrap().block_number,
            U64::from(second_block - safety_margin)
        );
    }
}
