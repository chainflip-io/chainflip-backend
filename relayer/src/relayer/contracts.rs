use thiserror::Error;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::Log,
    types::{BlockNumber, H256},
};

pub mod stake_manager;

pub use anyhow::Result;

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
