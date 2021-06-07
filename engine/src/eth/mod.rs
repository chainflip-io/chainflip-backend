mod stake_manager;

mod eth_event_streamer;

mod eth_broadcaster;

pub use anyhow::Result;
use async_trait::async_trait;
pub use eth_event_streamer::{EthEventStreamBuilder, EthEventStreamer};

use thiserror::Error;

use web3::types::{BlockNumber, FilterBuilder, H256};

use crate::settings::Settings;

#[async_trait]
pub trait Broadcast {
    // do we need to do any signing here? or do we *just* broadcast? let's go with the latter for now

    // TODO: this should return a Hash, not a String - but the hash needs to be generic across chains.
    // Do some chains not emit a hash? they may in the future so perhaps String is better?
    async fn broadcast(&self, msg: Vec<u8>) -> Result<String>;
}

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
    type Event: Send + Copy + Sync;

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

/// Start all the ETH components
pub async fn start(settings: Settings) -> anyhow::Result<()> {
    log::info!("Starting the ETH components");
    stake_manager::start_stake_manager_witness(settings)
        .await
        .expect("Could not start the StakeManager witness");

    // TODO: Start the eth broadcaster??
    // eth_broadcaster::run().await?;

    Ok(())
}
