use crate::{
    eth::{
        key_manager::{key_manager::KeyManager, key_manager_sink::KeyManagerSink},
        EthEventStreamBuilder,
    },
    mq::IMQClient,
    settings,
};

use anyhow::{Context, Result};

pub mod key_manager;
pub mod key_manager_sink;

/// Set up the eth event streamer for the KeyManager contract, and start it
pub async fn start_key_manager_witness<MQC: 'static + IMQClient + Send + Sync + Clone>(
    settings: &settings::Settings,
    mq_client: MQC,
) -> Result<()> {
    log::info!("Starting the Key Manager witness");

    EthEventStreamBuilder::new(
        settings.eth.node_endpoint.as_str(),
        KeyManager::load(settings.eth.key_manager_eth_address.as_str())?,
    )
    .with_sink(KeyManagerSink::<MQC>::new(mq_client).await?)
    .build()
    .await?
    .run(settings.eth.from_block.into())
    .await
    .context("Error occurred running the KeyManager events stream")?;

    Ok(())
}
