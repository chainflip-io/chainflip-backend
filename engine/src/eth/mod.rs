pub mod key_manager;
pub mod stake_manager;

mod eth_event_streamer;

mod eth_broadcaster;
mod eth_tx_encoding;

pub use anyhow::Result;
use async_trait::async_trait;
pub use eth_event_streamer::{EthEventStreamBuilder, EthEventStreamer};

use thiserror::Error;

use web3::types::{BlockNumber, FilterBuilder, H256};

use crate::{mq::IMQClient, settings::Settings};

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

/// Start all the ETH components
pub async fn start<MQC: 'static + IMQClient + Send + Sync + Clone>(
    settings: &Settings,
    mq_client: MQC,
    logger: &slog::Logger,
) {
    slog::info!(logger, "Starting the ETH components");
    let sm_witness_future =
        stake_manager::start_stake_manager_witness::<MQC>(&settings, mq_client.clone(), logger);

    let eth_broadcaster_future =
        eth_broadcaster::start_eth_broadcaster::<MQC>(&settings, mq_client.clone(), logger);

    let eth_tx_encoder_future =
        eth_tx_encoding::set_agg_key_with_agg_key::start(settings, mq_client.clone(), logger);

    let result = futures::join!(
        sm_witness_future,
        eth_broadcaster_future,
        eth_tx_encoder_future
    );
    result.0.expect("Broadcaster should exit without error");
    result.1.expect("Eth tx encoder should exit without error");
}
