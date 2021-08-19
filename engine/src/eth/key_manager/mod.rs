use crate::{eth::{key_manager::key_manager::KeyManager, eth_event_streamer}, logging::COMPONENT_KEY, settings};
use futures::Future;
use tokio::sync::mpsc::UnboundedSender;
use web3::{Web3, transports::WebSocket};
use anyhow::Result;
use slog::o;
use key_manager::KeyManagerEvent;

pub mod key_manager;

/// Set up the eth event streamer for the KeyManager contract, and start it
pub fn start_key_manager_witness(
    web3 : &Web3<WebSocket>,
    settings: &settings::Settings,
    sink : UnboundedSender<KeyManagerEvent>,
    logger: &slog::Logger,
) -> Result<impl Future> {
    let logger = logger.new(o!(COMPONENT_KEY => "KeyManagerWitness"));

    slog::info!(logger, "Starting KeyManager witness");

    let key_manager = KeyManager::new(&settings)?;

    Ok(eth_event_streamer::start(
        web3.clone(),
        key_manager.deployed_address,
        settings.eth.from_block,
        key_manager.parser_closure()?,
        sink,
        logger
    ))
}
