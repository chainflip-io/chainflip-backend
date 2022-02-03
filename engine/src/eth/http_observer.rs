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

const HTTP_POLL_INTERVAL: Duration = Duration::from_secs(4);

// TODO: Look into how providers generally handle reorgs on HTTP
// polls the HTTP endpoint every x seconds, returning the next head when it progresses
pub async fn polling_http_head_stream() -> impl Stream<Item = Block<H256>> {
    let transport = web3::transports::Http::new(
        "https://1ef2e10ce62d41a1a0741f8d84e91e3c.eth.rpc.rivet.cloud/",
    )
    .unwrap();

    struct StreamState {
        last_block_fetched: U64,
        last_block_yielded: U64,
        web3: Web3<Http>,
    }

    let init_data = StreamState {
        last_block_fetched: U64::from(0),
        last_block_yielded: U64::from(0),
        web3: web3::Web3::new(transport),
    };

    Box::pin(stream::unfold(init_data, move |mut state| async move {
        'block_safety_loop: loop {
            // TODO: Check this is the correct sleep
            std::thread::sleep(HTTP_POLL_INTERVAL);

            let unsafe_block_number = state.web3.eth().block_number().await.unwrap();
            println!("Got eth block number {}", unsafe_block_number);

            if unsafe_block_number < state.last_block_fetched {
                panic!("NOOO, we shouldn't ever go backwards on HTTP (I think, waiting for rivet to get back to me)");
            } else if unsafe_block_number == state.last_block_fetched {
                println!("Our unsafe block number is the same as the last fetched block");
                // ignore
            } else if unsafe_block_number == state.last_block_fetched + U64::from(1) {
                // we should progress to the next block
                println!("The unsafe block number is one above the last fetched");

                // TODO: Protect against overflows + check this is in aligment with the other observer
                // We should do this in a test
                // NB: The other observer will wait 5 blocks before going ahead, this one doesn't have to. Might as well kick off.
                let safe_block_number = unsafe_block_number - U64::from(ETH_BLOCK_SAFETY_MARGIN);

                // we can emit the block 5 less than this
                let block = state
                    .web3
                    .eth()
                    .block(safe_block_number.into())
                    .await
                    .unwrap()
                    .unwrap();
                state.last_block_yielded = block.number.unwrap();
                break Some((block, state));
            } else if unsafe_block_number > state.last_block_fetched + 1 {
                // we skipped a block
                // if our *head* is now at N, and we are assuming N - 5 is safe
                // Then (N - 1) - 5 must be safe
                let last_block_yielded_u64 = state.last_block_yielded.as_u64();
                for return_block_number in last_block_yielded_u64 + 1..unsafe_block_number.as_u64()
                {
                    let block = state
                        .web3
                        .eth()
                        .block(U64::from(return_block_number).into())
                        .await
                        .unwrap()
                        .unwrap();

                    state.last_block_yielded = block.number.unwrap();
                    break 'block_safety_loop Some((block, state));
                }
            }
            state.last_block_fetched = unsafe_block_number;
        }
    }))
}

mod tests {

    use super::*;

    #[tokio::test]
    async fn test_http_stream() {
        while let Some(block) = polling_http_head_stream().await.next().await {
            println!("Here's the block: {}", block.number.unwrap());
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

    #[tokio::test]
    async fn test_http_observer() {}
}
