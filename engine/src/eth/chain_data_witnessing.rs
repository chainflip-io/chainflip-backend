use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    logging::COMPONENT_KEY,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
    task_scope::{with_task_scope, ScopedJoinHandle},
};

use super::{rpc::EthRpcApi, EpochStart};

use anyhow::Context;
use cf_chains::{eth::TrackedData, Ethereum};
use futures::{future, Stream, StreamExt};
use slog::o;

use futures::FutureExt;
use slog::o;
use sp_core::{H256, U256};
use state_chain_runtime::CfeSettings;
use tokio::sync::{broadcast, watch};
use utilities::{context, make_periodic_tick};
use web3::types::{BlockNumber, U64};

pub const ETH_CHAIN_TRACKING_POLL_INTERVAL: Duration = Duration::from_secs(4);

pub async fn start<EthRpcClient, ScRpcClient>(
    eth_rpc: EthRpcClient,
    state_chain_client: Arc<StateChainClient<ScRpcClient>>,
    mut epoch_start_receiver: broadcast::Receiver<EpochStart>,
    cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
    poll_interval: Duration,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    EthRpcClient: 'static + EthRpcApi + Clone + Send + Sync,
    ScRpcClient: 'static + StateChainRpcApi + Send + Sync,
{
    with_task_scope(|scope| {
        async {
            let logger = logger.new(o!(COMPONENT_KEY => "ETH-Chain-Data-Witnesser"));
            slog::info!(&logger, "Starting");

            let mut last_observed_block_hash = None;
            let mut handle_and_end_observation_signal: Option<(ScopedJoinHandle<Option<H256>>, Arc<Mutex<Option<u64>>>)> = None;

            while let Ok(epoch_start) = epoch_start_receiver.recv().await {
                if let Some((handle, end_observation_signal)) = handle_and_end_observation_signal.take() {
                    *end_observation_signal.lock().unwrap() = Some(epoch_start.eth_block);
                    handle.await;
                }

                if epoch_start.participant && epoch_start.current {
                    handle_and_end_observation_signal = Some({
                        let end_observation_signal = Arc::new(Mutex::new(None));

                        // clone for capture by tokio task
                        let end_observation_signal_c = end_observation_signal.clone();
                        let eth_rpc_c = eth_rpc.clone();
                        let cfe_settings_update_receiver = cfe_settings_update_receiver.clone();
                        let state_chain_client = state_chain_client.clone();
                        let logger = logger.clone();

                        (
                            scope.spawn_with_handle::<_, _>(async move {
                                let mut poll_interval = make_periodic_tick(poll_interval, false);

                                loop {
                                    if let Some(_end_block) = *end_observation_signal.lock().unwrap() {
                                        break;
                                    }

                                    let block_number = eth_rpc_c.block_number().await?;
                                    let block_hash = eth_rpc_c.block(block_number).await?.hash.context(format!("Missing hash for block {}.", block_number))?;
                                    if last_observed_block_hash != Some(block_hash) {
                                        last_observed_block_hash = Some(block_hash);

                                        let priority_fee = cfe_settings_update_receiver
                                            .borrow()
                                            .eth_priority_fee_percentile;
                                        match get_tracked_data(&eth_rpc_c, block_number.as_u64(), priority_fee).await {
                                            Ok(tracked_data) => {
                                                state_chain_client
                                                    .submit_signed_extrinsic(
                                                        state_chain_runtime::Call::Witnesser(pallet_cf_witnesser::Call::witness {
                                                            call: Box::new(state_chain_runtime::Call::EthereumChainTracking(
                                                                pallet_cf_chain_tracking::Call::update_chain_state {
                                                                    state: tracked_data,
                                                                },
                                                            )),
                                                        }),
                                                        &logger,
                                                    )
                                                    .await
                                                    .context("Failed to submit signed extrinsic")?;
                                            }
                                            Err(e) => {
                                                slog::error!(&logger, "Failed to get tracked data: {:?}", e);
                                            }
                                        }
                                    }

                                    poll_interval.tick().await;
                                }

                                Ok(last_observed_block_hash)
                            }),
                            end_observation_signal_c
                        )
                    })
                }
            }

            slog::info!(&logger, "Stopping witnesser");
            Ok(())
        }
        .boxed()
    })
    .await
}

/// Queries the rpc node and builds the `TrackedData` for Ethereum at the requested block number.
///
/// Value in Wei is rounded to nearest Gwei in an effort to ensure agreement between nodes in the presence of floating
/// point / rounding error. This approach is still vulnerable when the true value is near the rounding boundary.
///
/// See: https://github.com/chainflip-io/chainflip-backend/issues/1803
async fn get_tracked_data<EthRpcClient: EthRpcApi + Send + Sync>(
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
