use crate::{
    eth::{
        stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink},
        EthEventStreamer,
    },
    mq::IMQClient,
    settings,
};
use web3::{Web3, DuplexTransport};

pub mod stake_manager;
pub mod stake_manager_sink;

use anyhow::Context;

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness<T : DuplexTransport, MQC: 'static + IMQClient + Send + Sync + Clone>(
    web3 : &Web3<T>,
    settings: &settings::Settings,
    mq_client: MQC,
    logger: &slog::Logger,
) {
    slog::info!(logger, "Starting StakeManager witness");

    EthEventStreamer::new(
        web3,
        StakeManager::load(settings.eth.stake_manager_eth_address.as_str())
            .expect("Should load StakeManager contract"),
        vec![StakeManagerSink::<MQC>::new(mq_client, logger)
            .await
            .expect("Should create StakeManagerSink")],
        logger,
    )
    .await
    .run(settings.eth.from_block.into())
    .await
    .context("Error occurred running the StakeManager events stream")
    .expect("Should run stake manager event stream");
}
