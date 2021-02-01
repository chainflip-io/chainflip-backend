pub mod contracts;
mod eth_event_streamer;
pub mod sinks;

pub use anyhow::Result;
pub use contracts::stake_manager::StakeManager;
pub use eth_event_streamer::EthEventStreamer;

use thiserror::Error;

use web3::{
    ethabi::TopicFilter,
    types::{BlockNumber, FilterBuilder},
};

/// The `Error` type for errors specific to this module.
#[derive(Error, Debug)]
pub enum RelayerError {}

/// Implement this for the substrate client.
#[async_trait]
pub trait EventSink<E>
where
    E: Send,
{
    async fn process_event(&self, event: E);
}

/// Implement this for the each contract.
pub trait EventSource {
    type Event: Send;

    fn topic_filter_for_event(&self, name: &str) -> Result<TopicFilter>;

    fn filter_builder(&self, block: BlockNumber) -> FilterBuilder;

    fn parse_event(&self, log: web3::types::Log) -> Result<Self::Event>;
}
