use std::{fmt, ops::Sub, pin::Pin};

use async_trait::async_trait;
use chainflip_common::types::coin::Coin;
use fmt::write;
use futures::Stream;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

/// Message Queue Result type
pub type Result<T> = anyhow::Result<T>;

/// Contains various general message queue options
pub struct Options {
    pub url: String,
}

/// Interface for a message queue
#[async_trait(?Send)]
pub trait IMQClient<M>
where
    M: Serialize + DeserializeOwned + Send,
{
    /// Open a connection to the message queue
    async fn connect(opts: Options) -> Result<Box<Self>>;

    /// Publish something to a particular subject
    async fn publish(&self, subject: Subject, message: M) -> Result<()>;

    /// Subscribe to a subject
    async fn subscribe(&self, subject: Subject) -> Result<Box<dyn Stream<Item = Vec<u8>>>>;

    // / Close the connection to the MQ
    async fn close(&self) -> Result<()>;
}

/// Used to pin a stream within a single scope.
pub fn pin_message_stream<M>(stream: Box<dyn Stream<Item = M>>) -> Pin<Box<dyn Stream<Item = M>>>
where
    M: Serialize + DeserializeOwned,
{
    stream.into()
}

/// Subjects that can be published / subscribed to
#[derive(Debug, Clone, Copy)]
pub enum Subject {
    Witness(Coin),
    Quote(Coin),
    Batch(Coin),
    Broadcast(Coin),
    Stake,
    Claim,
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            Subject::Witness(coin) => {
                write!(f, "witness.{}", coin.to_string())
            }
            Subject::Quote(coin) => {
                write!(f, "quote.{}", coin.to_string())
            }
            Subject::Batch(coin) => {
                write!(f, "batch.{}", coin.to_string())
            }
            Subject::Broadcast(coin) => {
                write!(f, "broadcast.{}", coin.to_string())
            }
            Subject::Stake => {
                write!(f, "stake")
            }
            Subject::Claim => {
                write!(f, "claim")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn channel_to_string() {
        let witness_subject = Subject::Witness(Coin::BTC);
        assert_eq!(witness_subject.to_string(), "witness.BTC");

        let quote_subject = Subject::Quote(Coin::ETH);
        assert_eq!(quote_subject.to_string(), "quote.ETH");

        let batch_subject = Subject::Batch(Coin::OXEN);
        assert_eq!(batch_subject.to_string(), "batch.OXEN");

        let broadcast_subject = Subject::Broadcast(Coin::BTC);
        assert_eq!(broadcast_subject.to_string(), "broadcast.BTC");

        let stake_subject = Subject::Stake;
        assert_eq!(stake_subject.to_string(), "stake");

        let claim_subject = Subject::Claim;
        assert_eq!(claim_subject.to_string(), "claim");
    }
}
