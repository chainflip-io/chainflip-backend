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

use anyhow::Context;

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness<MQC: 'static + IMQClient + Send + Sync + Clone>(
    settings: &settings::Settings,
    mq_client: MQC,
    logger: &slog::Logger,
) {
    slog::info!(logger, "Starting StakeManager witness");

    EthEventStreamBuilder::new(
        settings.eth.node_endpoint.as_str(),
        StakeManager::load(settings.eth.stake_manager_eth_address.as_str(), logger)
            .expect("Should load StakeManager contract"),
        logger,
    )
    .with_sink(
        StakeManagerSink::<MQC>::new(mq_client, logger)
            .await
            .expect("Should create StakeManagerSink"),
    )
    .build()
    .await
    .expect("Should build StakeManager stream")
    .run(settings.eth.from_block.into())
    .await
    .context("Error occurred running the StakeManager events stream")
    .expect("Should run stake manager event stream");
}
