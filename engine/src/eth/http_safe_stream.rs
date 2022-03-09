use std::{convert::TryInto, time::Duration};

use futures::{stream, Stream};
use web3::types::U64;

use crate::eth::EthHttpRpcApi;

pub const HTTP_POLL_INTERVAL: Duration = Duration::from_secs(4);

use super::{EthNumberBloom, EthRpcApi};

use anyhow::Result;

pub async fn safe_polling_http_head_stream<HttpRpc>(
    eth_http_rpc: HttpRpc,
    poll_interval: Duration,
    safety_margin: u64,
) -> impl Stream<Item = Result<EthNumberBloom>>
where
    HttpRpc: EthHttpRpcApi + EthRpcApi,
{
    struct StreamState<HttpRpc> {
        last_block_yielded: U64,
        last_head_fetched: U64,
        eth_http_rpc: HttpRpc,
    }

    let init_data = StreamState {
        last_block_yielded: U64::from(0),
        last_head_fetched: U64::from(0),
        eth_http_rpc,
    };

    Box::pin(stream::unfold(init_data, move |mut state| async move {
        loop {
            let is_first_iteration =
                state.last_block_yielded == U64::from(0) && state.last_head_fetched == U64::from(0);

            // Only request the latest block number if we are out of blocks to yield
            if state.last_head_fetched <= state.last_block_yielded + U64::from(safety_margin) {
                if !is_first_iteration {
                    tokio::time::sleep(poll_interval).await;
                }

                let unsafe_block_number = match state.eth_http_rpc.block_number().await {
                    Ok(block_number) => block_number,
                    Err(e) => break Some((Err(e), state)),
                };

                if unsafe_block_number + U64::from(safety_margin) < state.last_head_fetched {
                    break Some((Err(anyhow::Error::msg( format!("Fetched ETH block number ({}) is more than {} blocks behind the last fetched ETH block number ({})", unsafe_block_number, safety_margin, state.last_head_fetched))), state));
                }
                state.last_head_fetched = unsafe_block_number;
            }

            let next_block_to_yield = if is_first_iteration {
                state
                    .last_head_fetched
                    .saturating_sub(U64::from(safety_margin))
            } else {
                // the last block yielded was safe, so the next is +1
                state.last_block_yielded + U64::from(1)
            };
            if next_block_to_yield + U64::from(safety_margin) <= state.last_head_fetched {
                break Some((
                    state
                        .eth_http_rpc
                        .block(next_block_to_yield)
                        .await
                        .and_then(|block| block.try_into())
                        .map(|number_bloom: EthNumberBloom| {
                            state.last_block_yielded = number_bloom.block_number;
                            number_bloom
                        }),
                    state,
                ));
            }
        }
    }))
}

#[cfg(test)]
pub mod tests {

    use futures::StreamExt;
    use mockall::{predicate::eq, Sequence};
    use sp_core::H256;
    use web3::types::Block;
    use web3::types::H2048;

    use super::*;

    // in tests, this can be instant
    const TEST_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(1);

    use crate::constants::ETH_BLOCK_SAFETY_MARGIN;
    use crate::eth::mocks::MockEthHttpRpcClient;

    use anyhow::Result;

    pub fn dummy_block(block_number: u64) -> Result<Block<H256>> {
        Ok(Block {
            hash: Some(H256([(block_number % 256) as u8; 32])),
            number: Some(U64::from(block_number)),
            logs_bloom: Some(H2048::default()),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn returns_best_safe_block_immediately() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;
        let expected_returned_block_number = block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            expected_returned_block_number
        );
    }

    #[tokio::test]
    async fn does_not_return_until_chain_head_is_beyond_safety_margin() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

        let mut seq = Sequence::new();

        // we can't yield block 0
        let range = 1..=ETH_BLOCK_SAFETY_MARGIN + 1;
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
            .returning(move |n| dummy_block(n.as_u64()));

        let mut stream = safe_polling_http_head_stream(
            mock_eth_http_rpc_client,
            TEST_HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
        )
        .await;
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            U64::from(1)
        );
    }

    #[tokio::test]
    async fn does_not_return_block_until_progress() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;
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
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;
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
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;
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
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;
        for block_number in block_range {
            if let Some(block) = stream.next().await {
                assert_eq!(
                    block.unwrap().block_number,
                    U64::from(block_number - ETH_BLOCK_SAFETY_MARGIN)
                );
            };
        }
    }

    #[tokio::test]
    async fn return_error_on_bad_block_number_poll() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;

        for block_number in block_range {
            if let Some(block) = stream.next().await {
                assert_eq!(
                    block.unwrap().block_number,
                    U64::from(block_number - ETH_BLOCK_SAFETY_MARGIN)
                );
            };
        }

        assert!(stream.next().await.unwrap().is_err());
        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            U64::from(block_number_after_error - ETH_BLOCK_SAFETY_MARGIN)
        );
    }

    #[tokio::test]
    async fn return_error_on_good_block_number_bad_block_fetch_with_safety() {
        let mut mock_eth_http_rpc_client = MockEthHttpRpcClient::new();

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
        )
        .await;

        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            // no safety margin
            U64::from(first_block - safety_margin)
        );

        assert!(stream.next().await.unwrap().is_err());

        assert_eq!(
            stream.next().await.unwrap().unwrap().block_number,
            // no safety margin
            U64::from(second_block - safety_margin)
        );
    }
}
