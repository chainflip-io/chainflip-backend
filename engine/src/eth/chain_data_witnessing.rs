pub mod util;

use cf_chains::{eth::TrackedData, Ethereum};

use sp_core::U256;
use utilities::context;
use web3::types::{BlockNumber, U64};

use super::rpc::EthRpcApi;

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
}
