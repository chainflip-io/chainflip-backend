mod stake_manager;

mod eth_event_streamer;

pub use anyhow::Result;
use async_trait::async_trait;
pub use eth_event_streamer::{EthEventStreamBuilder, EthEventStreamer};

use thiserror::Error;

use web3::{
    contract::tokens::Tokenizable,
    ethabi::Log,
    types::{BlockNumber, FilterBuilder, H256},
};

/// The `Error` type for errors specific to this module.
#[derive(Error, Debug)]
pub enum RelayerError {}

/// Something that accepts and processes events asychronously.
#[async_trait]
pub trait EventSink<E>
where
    E: Send,
{
    /// Accepts an event and does something, returning a result to indicate success.
    async fn process_event(&self, event: E) -> Result<()>;
}

/// Implement this for each contract for which you want to subscribe to events.
pub trait EventSource {
    /// The Event type expected from this contract. Likely to be an enum of all possible events.
    type Event: Send + Copy;

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

pub fn decode_log_param<T: Tokenizable>(log: &Log, param_name: &str) -> Result<T> {
    let token = &log
        .params
        .iter()
        .find(|&p| p.name == param_name)
        .ok_or_else(|| EventProducerError::MissingParam(String::from(param_name)))?
        .value;

    Ok(Tokenizable::from_token(token.clone())?)
}
