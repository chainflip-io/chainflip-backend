use crate::{
    eth::{
        stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink},
        EthEventStreamBuilder,
    },
    mq::IMQClient,
    settings,
};

pub mod stake_manager;
pub mod stake_manager_sink;

use anyhow::{Context, Result};

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness<MQC: 'static + IMQClient + Send + Sync + Clone>(
    settings: &settings::Settings,
    mq_client: MQC,
    logger: &slog::Logger,
) -> Result<()> {
    slog::info!(logger, "Starting!");
    let stake_manager =
        StakeManager::load(settings.eth.stake_manager_eth_address.as_str(), logger)?;

    let sm_sink = StakeManagerSink::<MQC>::new(mq_client, logger).await?;
    let sm_event_stream =
        EthEventStreamBuilder::new(settings.eth.node_endpoint.as_str(), stake_manager, logger);
    let sm_event_stream = sm_event_stream.with_sink(sm_sink).build().await?;
    sm_event_stream
        .run(settings.eth.from_block.into())
        .await
        .context("Error occurred running the StakeManager events stream")?;
    Ok(())
}
