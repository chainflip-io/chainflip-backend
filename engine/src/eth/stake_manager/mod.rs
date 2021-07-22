use crate::{
    eth::{
        stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink},
        EthEventStreamBuilder,
    },
    mq::{
        nats_client::{NatsMQClient, NatsMQClientFactory},
        IMQClientFactory,
    },
    settings,
};

pub mod stake_manager;
pub mod stake_manager_sink;

use anyhow::{Context, Result};

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_witness(settings: settings::Settings) -> Result<()> {
    log::info!("Starting the stake manager witness");
    let stake_manager = StakeManager::load(settings.eth.stake_manager_eth_address.as_str())?;

    log::info!("Create new NatsMQClientFactory");
    let factory = NatsMQClientFactory::new(&settings.message_queue);
    log::info!("Create new NatsMQClient");
    let mq_client = *factory.create().await?;

    log::info!("Create new StakeManagerSink");
    let sm_sink = StakeManagerSink::<NatsMQClient>::new(mq_client).await?;
    let sm_event_stream =
        EthEventStreamBuilder::new(settings.eth.node_endpoint.as_str(), stake_manager);
    let sm_event_stream = sm_event_stream.with_sink(sm_sink).build().await?;
    log::info!("Start running SM Event stream");
    sm_event_stream
        .run(Some(settings.eth.from_block))
        .await
        .context("Error occurred running the StakeManager events stream")?;
    Ok(())
}
