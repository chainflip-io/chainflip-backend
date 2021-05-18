use crate::eth::stake_manager::stake_manager::StakeManager;
use crate::eth::stake_manager::stake_manager_sink::StakeManagerSink;
use crate::eth::EthEventStreamBuilder;
use crate::mq::nats_client::NatsMQClient;
use crate::mq::Options;
use crate::settings;

pub mod stake_manager;
pub mod stake_manager_sink;

use anyhow::{Context, Result};

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness(settings: settings::Settings) -> Result<()> {
    log::info!("Starting the stake manager witness");

    let stake_manager = StakeManager::load(settings.eth.stake_manager_eth_address.as_str())?;

    let mq_options = Options {
        url: format!(
            "{}:{}",
            settings.message_queue.hostname, settings.message_queue.port
        ),
    };
    let sm_sink = StakeManagerSink::<NatsMQClient>::new(mq_options).await?;
    let eth_node_ws_url = format!("ws://{}:{}", settings.eth.hostname, settings.eth.port);
    let sm_event_stream = EthEventStreamBuilder::new(eth_node_ws_url.as_str(), stake_manager);
    let sm_event_stream = sm_event_stream.with_sink(sm_sink).build().await?;
    sm_event_stream
        .run(Some(0))
        .await
        .context("Error occurred running the StakeManager events stream")?;
    Ok(())
}
