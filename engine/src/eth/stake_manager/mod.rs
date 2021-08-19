use crate::{
    eth::{eth_event_streamer, stake_manager::stake_manager::StakeManager},
    mq::IMQClient,
    settings,
};
use web3::{Web3, DuplexTransport};

pub mod stake_manager;

use anyhow::Context;

/// Set up the eth event streamer for the StakeManager contract, and start it
pub async fn start_stake_manager_observer<EventSink : Sink<Event>, T : DuplexTransport, MQC: 'static + IMQClient + Send + Sync + Clone>(
    web3 : &Web3<T>,
    settings: &settings::Settings,
    sink : EventSink,
    logger: &slog::Logger,
) {
    slog::info!(logger, "Starting StakeManager obverser");

    eth_event_streamer::start(
        web3,
        H160::from_str(&settings.eth.stake_manager_eth_address)?,
        settings.eth.from_block,
        |a, b, c| {c},
        sink,
        logger
    ).await
}
