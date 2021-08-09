use crate::{
    eth::{
        stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink},
        EthEventStreamer,
    },
    mq::IMQClient,
    settings,
};

pub mod stake_manager;
pub mod stake_manager_sink;

use anyhow::Context;

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness<MQC: 'static + IMQClient + Send + Sync + Clone>(
    settings: &settings::Settings,
    mq_client: MQC,
    logger: &slog::Logger,
) {
    slog::info!(logger, "Starting StakeManager witness");

    EthEventStreamer::new(
        &settings.eth.node_endpoint,
        StakeManager::load(settings.eth.stake_manager_eth_address.as_str(), logger)
            .expect("Should load StakeManager contract"),
        vec![StakeManagerSink::<MQC>::new(mq_client, logger)
            .await
            .expect("Should create StakeManagerSink")],
        logger,
    )
    .await
    .expect("Should build EthEventStreamer")
    .run(settings.eth.from_block.into())
    .await
    .context("Error occurred running the StakeManager events stream")
    .expect("Should run stake manager event stream");
}
