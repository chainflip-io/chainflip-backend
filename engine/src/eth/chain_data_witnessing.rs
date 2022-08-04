pub mod util;

use std::time::Duration;

use crate::logging::COMPONENT_KEY;

use super::rpc::EthRpcApi;

use cf_chains::{eth::TrackedData, Ethereum};
use futures::{future, Stream, StreamExt};
use slog::o;

use sp_core::U256;
use utilities::{context, periodic_tick_stream};
use web3::types::{BlockNumber, U64};

/// Returns a stream of latest eth block numbers by polling at regular intervals.
///
/// Uses polling.
pub fn poll_latest_block_numbers<'a, EthRpc: EthRpcApi + Send + Sync + 'a>(
    eth_rpc: &'a EthRpc,
    polling_interval: Duration,
    logger: &slog::Logger,
) -> impl Stream<Item = u64> + 'a {
    let logger = logger.new(o!(COMPONENT_KEY => "ETH_Poll_LatestBlockStream"));

    periodic_tick_stream(polling_interval)
        .then(move |_| eth_rpc.block_number())
        .filter_map(move |rpc_result| {
            future::ready(match rpc_result {
                Ok(block_number) => Some(block_number.as_u64()),
                Err(e) => {
                    slog::warn!(logger, "Error fetching ETH block number: {}", e);
                    None
                }
            })
        })
}

/// Queries the rpc node and builds the `TrackedData` for Ethereum at the requested block number.
///
/// Value in Wei is rounded to nearest Gwei in an effort to ensure agreement between nodes in the presence of floating
/// point / rounding error. This approach is still vulnerable when the true value is near the rounding boundary.
///
/// See: https://github.com/chainflip-io/chainflip-backend/issues/1803
pub async fn get_tracked_data<EthRpcClient: EthRpcApi + Send + Sync>(
    rpc: &EthRpcClient,
    block_number: u64,
    priority_fee_percentile: u8,
) -> anyhow::Result<TrackedData<Ethereum>> {
    let fee_history = rpc
        .fee_history(
            U256::one(),
            BlockNumber::Number(U64::from(block_number)),
            Some(vec![priority_fee_percentile as f64 / 100_f64]),
        )
        .await?;

    Ok(TrackedData::<Ethereum> {
        block_height: block_number,
        base_fee: context!(fee_history.base_fee_per_gas.first())?.as_u128(),
        priority_fee: context!(context!(context!(fee_history.reward)?.first())?.first())?.as_u128(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::test_utils::new_test_logger;

    #[tokio::test]
    async fn test_get_tracked_data() {
        use crate::eth::rpc::MockEthRpcApi;

        const BLOCK_HEIGHT: u64 = 42;
        const BASE_FEE: u128 = 40_000_000_000;
        const PRIORITY_FEE: u128 = 5_000_000_000;

        let mut rpc = MockEthRpcApi::new();

        // ** Rpc Api Assumptions **
        rpc.expect_fee_history()
            .once()
            .returning(|_, block_number, _| {
                Ok(web3::types::FeeHistory {
                    oldest_block: block_number,
                    base_fee_per_gas: vec![U256::from(BASE_FEE)],
                    gas_used_ratio: vec![],
                    reward: Some(vec![vec![U256::from(PRIORITY_FEE)]]),
                })
            });
        // ** Rpc Api Assumptions **

        assert_eq!(
            get_tracked_data(&rpc, BLOCK_HEIGHT, 50).await.unwrap(),
            TrackedData {
                block_height: BLOCK_HEIGHT,
                base_fee: BASE_FEE,
                priority_fee: PRIORITY_FEE,
            }
        );
    }

    #[tokio::test]
    async fn test_poll_latest_block_numbers() {
        use crate::eth::rpc::MockEthRpcApi;

        const BLOCK_COUNT: u64 = 10;
        let mut block_numbers = (0..BLOCK_COUNT).map(Into::into);

        let mut rpc = MockEthRpcApi::new();
        let logger = new_test_logger();

        // ** Rpc Api Assumptions **
        rpc.expect_block_number()
            .times(BLOCK_COUNT as usize)
            .returning(move || {
                block_numbers
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("No more block numbers"))
            });
        // ** Rpc Api Assumptions **

        assert_eq!(
            poll_latest_block_numbers(&rpc, Duration::from_millis(10), &logger)
                .take(BLOCK_COUNT as usize)
                .collect::<Vec<_>>()
                .await,
            (0..BLOCK_COUNT).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_poll_latest_block_numbers_skips_errors() {
        use crate::eth::rpc::MockEthRpcApi;

        const REQUEST_COUNT: usize = 10;
        const BLOCK_COUNT: usize = 8;

        let mut rpc = MockEthRpcApi::new();
        let logger = new_test_logger();

        // ** Rpc Api Assumptions **
        // Simulates a realistic infura sever: one in five requests errors.
        let mut req_number = 0;
        let mut block_number = 0;
        rpc.expect_block_number()
            .times(REQUEST_COUNT as usize)
            .returning(move || {
                let result = if req_number % 5 == 0 {
                    Err(anyhow::anyhow!("Infura says no."))
                } else {
                    let res = Ok(block_number.into());
                    block_number += 1;
                    res
                };
                req_number += 1;
                result
            });
        // ** Rpc Api Assumptions **

        assert_eq!(
            poll_latest_block_numbers(&rpc, Duration::from_millis(10), &logger)
                .take(BLOCK_COUNT)
                .collect::<Vec<_>>()
                .await,
            (0..BLOCK_COUNT as u64).collect::<Vec<_>>()
        );
    }
}
