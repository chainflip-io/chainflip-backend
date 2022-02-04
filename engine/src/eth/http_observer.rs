use std::collections::VecDeque;
use std::time::Duration;

use futures::StreamExt;
use futures::{stream, Stream};
use sp_core::H256;
use web3::{
    transports::Http,
    types::{Block, U64},
    Web3,
};

use crate::constants::ETH_BLOCK_SAFETY_MARGIN;

use super::EthHttpRpcApi;

const HTTP_POLL_INTERVAL: Duration = Duration::from_secs(4);

// TODO: Look into how providers generally handle reorgs on HTTP
// polls the HTTP endpoint every x seconds, returning the next head when it progresses
pub async fn polling_http_head_stream<EthHttpRpc: EthHttpRpcApi>(
    eth_http_rpc: EthHttpRpc,
    poll_interval: Duration,
) -> impl Stream<Item = Block<H256>> {
    struct StreamState<EthHttpRpc> {
        last_block_fetched: U64,
        last_block_yielded: U64,
        cached_skipped_blocks: VecDeque<Block<H256>>,
        eth_http_rpc: EthHttpRpc,
    }

    let init_data = StreamState {
        last_block_fetched: U64::from(0),
        last_block_yielded: U64::from(0),
        cached_skipped_blocks: VecDeque::default(),
        eth_http_rpc,
    };

    Box::pin(stream::unfold(init_data, move |mut state| async move {
        'block_safety_loop: loop {
            // we first want to empty the cache of skipped blocks before querying for new ones
            while let Some(block) = state.cached_skipped_blocks.pop_front() {
                println!("Popping off the queue block: {}", block.number.unwrap());
                break 'block_safety_loop Some((block, state));
            }

            // TODO: Check this is the correct sleep
            std::thread::sleep(poll_interval);

            println!("About to query for block number");
            let unsafe_block_number = state.eth_http_rpc.block_number().await.unwrap();
            let safe_block_number = unsafe_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
            println!("Got eth block number {}", unsafe_block_number);

            if unsafe_block_number < state.last_block_fetched {
                panic!("NOOO, we shouldn't ever go backwards on HTTP (I think, waiting for rivet to get back to me)");
            } else if unsafe_block_number == state.last_block_fetched {
                println!("Our unsafe block number is the same as the last fetched block");
                // ignore - we will wait until next poll to go again
            } else if (state.last_block_yielded == U64::from(0)
                && state.last_block_fetched == U64::from(0))
                || unsafe_block_number == state.last_block_fetched + U64::from(1)
            {
                // We enter this when we have progressed, or if this is the first iteration
                // we should progress to the next block
                println!(
                    "The unsafe block number is {} and last fetched is: {}",
                    unsafe_block_number, state.last_block_fetched
                );

                // TODO: Protect against overflows + check this is in aligment with the other observer
                // We should do this in a test
                // NB: The other observer will wait 5 blocks before going ahead, this one doesn't have to. Might as well kick off.

                println!(
                    "the unsafe block number of: {} means safe number is: {}",
                    unsafe_block_number, safe_block_number
                );

                // we can emit the block 5 less than this
                let block = state
                    .eth_http_rpc
                    .block(safe_block_number.into())
                    .await
                    .unwrap()
                    .unwrap();
                state.last_block_fetched = unsafe_block_number;
                state.last_block_yielded = block.number.unwrap();
                break Some((block, state));
                // we want to skip the first block

                // check last yielded too?
            } else if state.last_block_fetched != U64::from(0)
                && unsafe_block_number > state.last_block_fetched + 1
            {
                // We should never run this on the first go round
                assert!(state.last_block_fetched != U64::from(0));
                // we skipped a block
                // if our *head* is now at N, and we are assuming N - 5 is safe
                // Then (N - 1) - 5 must be safe
                let last_block_yielded_u64 = state.last_block_yielded.as_u64();
                for return_block_number in last_block_yielded_u64 + 1..safe_block_number.as_u64() {
                    println!("Adding block number: {} to the cache", return_block_number);
                    let block = state
                        .eth_http_rpc
                        .block(U64::from(return_block_number).into())
                        .await
                        .unwrap()
                        .unwrap();

                    state.last_block_yielded = block.number.unwrap();
                    state.cached_skipped_blocks.push_back(block);
                }

                println!("Yielding block");
                state.last_block_fetched = unsafe_block_number;
                break 'block_safety_loop Some((
                    state
                        .cached_skipped_blocks
                        .pop_front()
                        .expect("There must be a block here, as we must have pushed at least one item to the queue"),
                    state,
                ));
            } else {
                println!(
                    "Entered else. Unsafe block: {}. last fetched: {}",
                    unsafe_block_number, state.last_block_fetched
                );
                state.last_block_fetched = unsafe_block_number;
            }
        }
    }))
}

#[cfg(test)]
mod tests {

    use mockall::{mock, predicate::eq, Sequence};

    use crate::eth::{EthHttpRpcApi, EthRpcApi};

    use super::*;

    // in tests, this can be instant
    const TEST_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(1);

    use crate::eth::mocks::MockEthHttpRpc;
    use async_trait::async_trait;

    use anyhow::Result;

    fn dummy_block(block_number: u64) -> Result<Option<Block<H256>>> {
        Ok(Some(Block {
            hash: Some(H256([(block_number % 256) as u8; 32])),
            number: Some(U64::from(block_number)),
            logs_bloom: Default::default(),
            ..Default::default()
        }))
    }

    // TODO empty stream?

    #[tokio::test]
    async fn returns_best_safe_block_immediately() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        let block_number = U64::from(10);
        mock_eth_http_rpc
            .expect_block_number()
            .times(1)
            .returning(move || Ok(block_number));

        mock_eth_http_rpc
            .expect_block()
            .times(1)
            .returning(move |n| dummy_block(n.as_u64()));

        let mut stream = polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL).await;
        let expected_returned_block_number = block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().number.unwrap(),
            expected_returned_block_number
        );
    }

    #[tokio::test]
    async fn does_not_return_block_until_progress() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

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

        let mut stream = polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL).await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().number.unwrap(),
            expected_first_returned_block_number
        );
        let expected_next_returned_block_number =
            next_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().number.unwrap(),
            expected_next_returned_block_number
        );
    }

    #[tokio::test]
    async fn catches_up_if_polling_skipped_a_block_number() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

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

        // if we skip blocks, we should catch up
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
        let mut stream = polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL).await;
        let expected_first_returned_block_number =
            first_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);
        assert_eq!(
            stream.next().await.unwrap().number.unwrap(),
            expected_first_returned_block_number
        );

        println!("First block returned as expected");

        println!("The range is: {:?}", skipped_range);
        // we should get all the skipped blocks next (that are within the safety margin)
        for n in skipped_range {
            println!("This is n: {}", n);
            let expected_skipped_block_number = U64::from(n - ETH_BLOCK_SAFETY_MARGIN);
            assert_eq!(
                stream.next().await.unwrap().number.unwrap(),
                expected_skipped_block_number
            );
        }
    }

    #[tokio::test]
    async fn if_block_numbers_increment_by_one_progresses_at_block_margin() {
        let mut mock_eth_http_rpc = MockEthHttpRpc::new();

        // ensure these looped calls occur in order
        let mut seq_numbers = Sequence::new();

        // TODO: Try unify these
        let mut seq_blocks = Sequence::new();

        let block_range = 10..20;

        for block_number in block_range.clone() {
            mock_eth_http_rpc
                .expect_block_number()
                .times(1)
                .in_sequence(&mut seq_numbers)
                .returning(move || Ok(U64::from(block_number)));

            mock_eth_http_rpc
                .expect_block()
                .times(1)
                .in_sequence(&mut seq_blocks)
                .returning(move |number| dummy_block(number.as_u64()));
        }

        let mut stream = polling_http_head_stream(mock_eth_http_rpc, TEST_HTTP_POLL_INTERVAL).await;
        for block_number in block_range {
            println!("Testing block_number: {}", block_number);
            if let Some(block) = stream.next().await {
                assert_eq!(
                    block.number.unwrap(),
                    U64::from(block_number - ETH_BLOCK_SAFETY_MARGIN)
                );
            };
        }
    }

    #[tokio::test]
    async fn run_http_eth_shit() {
        let transport = web3::transports::Http::new(
            "https://1ef2e10ce62d41a1a0741f8d84e91e3c.eth.rpc.rivet.cloud/",
        )
        .unwrap();

        let web3 = web3::Web3::new(transport);

        for _ in 1..10 {
            let block_number = web3.eth().block_number().await.unwrap();
            println!("Got eth block number {}", block_number);

            // let block_id = block_number;
            let block = web3
                .eth()
                .block(block_number.into())
                .await
                .unwrap()
                .unwrap();

            println!("Here's the block number from the block: {:?}", block.number);

            println!("Here's the txs from the block: {:?}", block.transactions);
            // let head = web3.eth().(block_number);
            std::thread::sleep(Duration::from_secs(6));

            // let logs = web3.eth().logs().await;
        }
    }
}
