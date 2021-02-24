pub mod contracts;
mod eth_event_streamer;
pub mod sinks;

pub use anyhow::Result;
use async_trait::async_trait;
pub use contracts::stake_manager::StakeManager;
pub use eth_event_streamer::{EthEventStreamBuilder, EthEventStreamer};

use thiserror::Error;

use web3::types::{BlockNumber, FilterBuilder};

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
