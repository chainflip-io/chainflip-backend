pub mod key_manager;
pub mod stake_manager;

mod eth_event_streamer;

pub mod eth_broadcaster;
pub mod eth_tx_encoding;
pub mod utils;

use anyhow::Result;
use async_trait::async_trait;
pub use eth_event_streamer::EthEventStreamer;

use thiserror::Error;

use std::time::Duration;
use crate::settings;
use web3::types::{BlockNumber, FilterBuilder, H256};
use web3;

/// Something that accepts and processes events asychronously.
#[async_trait]
pub trait EventSink<E>
where
    E: Send + Sync,
{
    /// Accepts an event and does something, returning a result to indicate success.
    async fn process_event(&self, event: E) -> Result<()>;
}

/// Implement this for each contract for which you want to subscribe to events.
pub trait EventSource {
    /// The Event type expected from this contract. Likely to be an enum of all possible events.
    type Event: Clone + Send + Sync + std::fmt::Debug;

    /// Returns an eth filter for the events from the contract, starting at the given
    /// block number.
    fn filter_builder(&self, block: BlockNumber) -> FilterBuilder;

    /// Attempt to parse an event from an ethereum Log item.
    fn parse_event(&self, log: web3::types::Log) -> Result<Self::Event>;
}

/// The `Error` type for errors specific to this module.
#[derive(Error, Debug)]
pub enum EventProducerError {
    #[error("Unexpected event signature in log subscription: {0:#}")]
    UnexpectedEvent(H256),

    /// A log was received with an empty "topics" vector, shouldn't happen.
    #[error("Expected log to contain topics, got empty vector.")]
    EmptyTopics,

    /// Tried to decode a parameter that doesn't exist in the log.
    #[error("Cannot decode missing parameter: '{0}'.")]
    MissingParam(String),
}

pub async fn new_web3_client(settings : &settings::Settings, logger : &slog::Logger) -> Result<web3::Web3<web3::transports::WebSocket>> {
    let node_endpoint = &settings.eth.node_endpoint;
    slog::debug!(
        logger,
        "Connecting new web3 client to {}",
        node_endpoint
    );
    match tokio::time::timeout(
        Duration::from_secs(5),
        async {
            Ok(web3::Web3::new(web3::transports::WebSocket::new(node_endpoint).await?))
        },
    ).await {
        Ok(result) => result,
        Err(_) => {
            // Connection timeout
            Err(anyhow::Error::msg(format!(
                "Timeout connecting to {:?}",
                node_endpoint
            )))
        }
    }

}