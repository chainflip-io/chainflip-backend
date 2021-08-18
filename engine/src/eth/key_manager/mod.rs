use crate::{
    eth::{
        key_manager::{key_manager::KeyManager, key_manager_sink::KeyManagerSink},
        EthEventStreamer,
    },
    mq::IMQClient,
    settings,
};

use anyhow::Context;

pub mod key_manager;
pub mod key_manager_sink;

/// Set up the eth event streamer for the KeyManager contract, and start it
pub async fn start_key_manager_witness<MQC: 'static + IMQClient + Send + Sync + Clone>(
    settings: &settings::Settings,
    mq_client: MQC,
    logger: &slog::Logger,
) {
    slog::info!(logger, "Starting KeyManager witness");

    EthEventStreamer::new(
        &settings.eth.node_endpoint,
        KeyManager::load(settings.eth.key_manager_eth_address.as_str())
            .expect("Should load KeyManager contract"),
        vec![KeyManagerSink::<MQC>::new(mq_client, logger)
            .await
            .expect("Should create KeyManagerSink")],
        logger,
    )
    .await
    .expect("Streamer should be created")
    .run(settings.eth.from_block.into())
    .await
    .context("Error occurred running the KeyManager events stream")
    .expect("Should run key manager event stream");
}
