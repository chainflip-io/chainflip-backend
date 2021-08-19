use crate::{eth::{eth_event_streamer, stake_manager::stake_manager::StakeManager}, logging::COMPONENT_KEY, settings};
use futures::Future;
use slog::o;
use tokio::sync::mpsc::UnboundedSender;
use web3::{Web3, transports::WebSocket};

pub mod stake_manager;

use anyhow::Result;

use stake_manager::StakeManagerEvent;

/// Set up the eth event streamer for the StakeManager contract, and start it
pub fn start_stake_manager_witness(
    web3 : &Web3<WebSocket>,
    settings: &settings::Settings,
    sink : UnboundedSender<StakeManagerEvent>,
    logger: &slog::Logger,
) -> Result<impl Future> {
    let logger = logger.new(o!(COMPONENT_KEY => "StakeManagerWitness"));

    slog::info!(logger, "Starting StakeManager obverser");

    let stake_manager = StakeManager::new(&settings)?;

    Ok(eth_event_streamer::start(
        web3.clone(),
        stake_manager.deployed_address,
        settings.eth.from_block,
        stake_manager.parser_closure()?,
        sink,
        logger
    ))
}
