use thiserror::Error;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::{Log, TopicFilter},
    types::{BlockNumber, FilterBuilder, H256},
};

pub mod stake_manager;

pub type Result<T> = core::result::Result<T, EventProducerError>;

/// The `Error` type for this module.
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

    /// Wrapper for `web3::contract` errors.
    #[error(transparent)]
    DetokenizerError(#[from] web3::contract::Error),

    /// Wrapper for `web3::ethabi` errors.
    #[error(transparent)]
    AbiError(#[from] web3::ethabi::Error),

    /// Wrapper for other `web3` errors.
    #[error(transparent)]
    Web3Error(#[from] web3::Error),

    /// Represents all other cases of `std::io::Error`.
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

/// Implement this for the each contract.
pub trait EventSource {
    type Event: Send;
    type Error: std::fmt::Debug;

    fn topic_filter_for_event(&self, name: &str) -> Result<TopicFilter>;

    fn filter_builder(&self, block: BlockNumber) -> FilterBuilder;

    fn parse_event(&self, log: web3::types::Log) -> Result<Self::Event>;
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

// pub async fn event_stream<E: EventSource, T: DuplexTransport>(
//     web3_client: &Web3<T>,
//     event_source: &E,
//     from_block: BlockNumber,
// ) -> Result<dyn Stream<Item = E::Event>> {
//     let filter = event_source.filter_builder(from_block).build();
//     let stream = web3_client.eth_subscribe().subscribe_logs(filter).await?;

//     Ok(stream.map(|log| log.map(event_source.parse_event)))
// }
