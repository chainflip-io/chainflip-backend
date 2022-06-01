use std::{sync::Arc, time::Duration};

use crate::{
    logging::COMPONENT_KEY,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};

use super::rpc::{EthHttpRpcApi, EthRpcApi};

use cf_chains::eth::TrackedData;
use futures::{future, sink, stream, Stream, StreamExt};
use slog::o;
use sp_core::U256;
use web3::types::{BlockNumber, U64};

pub const ETH_CHAIN_TRACKING_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Returns a stream that yields `()` at regular intervals.
fn tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
    stream::unfold(tokio::time::interval(tick_interval), |mut interval| async {
        interval.tick().await;
        Some(((), interval))
    })
}

/// Returns a stream of latest eth block numbers.
pub fn latest_block_numbers<'a, HttpRpc: EthHttpRpcApi + Send + Sync>(
    eth_http_rpc: &'a HttpRpc,
    polling_interval: Duration,
    logger: &slog::Logger,
) -> impl Stream<Item = U64> + 'a {
    let logger = logger.new(o!(COMPONENT_KEY => "ETH_HTTPLatestBlockStream"));

    tick_stream(polling_interval)
        .then(move |_| async move { eth_http_rpc.block_number().await })
        .filter_map(move |rpc_result| {
            future::ready(match rpc_result {
                Ok(block_number) => Some(block_number),
                Err(e) => {
                    slog::warn!(logger, "Error fetching ETH block number: {}", e);
                    None
                }
            })
        })
        .scan(U64::default(), |last, latest| {
            future::ready(Some(if *last != latest {
                *last = latest;
                Some(latest)
            } else {
                None
            }))
        })
        .filter_map(|x| future::ready(x))
}

pub async fn chain_data_witnesser<
    HttpRpc: EthHttpRpcApi + EthRpcApi + Send + Sync,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
>(
    eth_http_rpc: &HttpRpc,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    logger: &slog::Logger,
) {
    let mut client_sink = sink::unfold::<_, _, _, _, anyhow::Error>(
        state_chain_client,
        |mut client, call| async move {
            client.submit_signed_extrinsic(call, logger).await?;
            Ok(client)
        },
    );

    latest_block_numbers(eth_http_rpc, ETH_CHAIN_TRACKING_POLL_INTERVAL, logger)
        .then(move |block_number| async move {
            let fee_history = eth_http_rpc
                .fee_history(
                    U256::one(),
                    BlockNumber::Number(block_number),
                    Some(vec![0.5]),
                )
                .await?;

            Ok(state_chain_runtime::Call::EthereumChainTracking(
                pallet_cf_chain_tracking::Call::update_chain_state {
                    state: TrackedData::<cf_chains::Ethereum> {
                        block_height: block_number.as_u64(),
                        base_fee: fee_history
                            .base_fee_per_gas
                            .first()
                            .expect("Requested, so should be present.")
                            .as_u128(),
                        priority_fee: fee_history
                            .reward
                            .expect("Requested, so should be present.")
                            .first()
                            .expect("Requested, so should be present.")
                            .first()
                            .expect("Requested, so should be present.")
                            .as_u128(),
                    },
                },
            ))
        })
        .forward(client_sink)
        .await;
}
