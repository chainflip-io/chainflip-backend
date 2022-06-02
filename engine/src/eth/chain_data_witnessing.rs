pub mod http;
pub mod ws;

use std::time::Duration;

use super::rpc::{EthHttpRpcApi, EthRpcApi, EthWsRpcApi};

use cf_chains::{eth::TrackedData, Ethereum};
use futures::{
    future,
    stream::{self, BoxStream},
    Stream, StreamExt,
};

use sp_core::U256;
use web3::types::{BlockNumber, U64};

/// Returns a stream that yields `()` at regular intervals.
fn tick_stream(tick_interval: Duration) -> impl Stream<Item = ()> {
    stream::unfold(tokio::time::interval(tick_interval), |mut interval| async {
        interval.tick().await;
        Some(((), interval))
    })
}

fn bounded<'a>(
    from_block: u64,
    to_block_receiver: tokio::sync::watch::Receiver<Option<u64>>,
    block_numbers: impl Stream<Item = u64> + Send + 'a,
) -> BoxStream<'a, u64> {
    Box::pin(
        block_numbers
            .skip_while(move |block_number| future::ready(*block_number < from_block))
            .take_while(move |block_number| {
                future::ready(
                    to_block_receiver
                        .borrow()
                        .map(|to_block| *block_number <= to_block)
                        .unwrap_or(true),
                )
            }),
    )
}

pub async fn http_chain_data_witnesser<'a, EthRpcClient>(
    from_block: u64,
    to_block_receiver: tokio::sync::watch::Receiver<Option<u64>>,
    polling_interval: Duration,
    eth_rpc: &'a EthRpcClient,
    logger: &slog::Logger,
) -> impl Stream<Item = anyhow::Result<TrackedData<Ethereum>>> + 'a
where
    EthRpcClient: EthHttpRpcApi + EthRpcApi + Send + Sync,
{
    chain_data_witnesser(
        bounded(
            from_block,
            to_block_receiver,
            http::latest_block_numbers(eth_rpc, polling_interval, logger),
        ),
        eth_rpc,
    )
    .await
}

pub async fn ws_chain_data_witnesser<'a, EthRpcClient>(
    from_block: u64,
    to_block_receiver: tokio::sync::watch::Receiver<Option<u64>>,
    eth_rpc: &'a EthRpcClient,
    logger: &slog::Logger,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<TrackedData<Ethereum>>> + 'a>
where
    EthRpcClient: EthWsRpcApi + EthRpcApi + Send + Sync,
{
    Ok(chain_data_witnesser(
        bounded(
            from_block,
            to_block_receiver,
            ws::latest_block_numbers(eth_rpc, logger).await?,
        ),
        eth_rpc,
    )
    .await)
}

async fn chain_data_witnesser<'a, EthRpcClient: EthRpcApi + Send + Sync>(
    block_numbers: impl Stream<Item = u64> + 'a,
    eth_rpc: &'a EthRpcClient,
) -> impl Stream<Item = anyhow::Result<TrackedData<Ethereum>>> + 'a {
    block_numbers.then(move |block_number| async move {
        let fee_history = eth_rpc
            .fee_history(
                U256::one(),
                BlockNumber::Number(U64::from(block_number)),
                Some(vec![0.5]),
            )
            .await?;

        Ok(TrackedData::<Ethereum> {
            block_height: block_number,
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
        })
    })
}
