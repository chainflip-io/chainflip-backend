use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;

// use super::nats_client::NatsReceiverAdapter;

/// Message should be deserialized by the individual components
#[derive(Debug, PartialEq, Clone)]
pub struct Message(pub Vec<u8>);

/// Message Queue Result type
pub type Result<T> = std::result::Result<T, MQError>;

/// Contains various general message queue options
pub struct Options {
    pub url: &'static str,
}

/// Message Queue Error type
#[derive(Error, Debug)]
pub enum MQError {
    /// Failure to publish to the subject
    #[error("Error publishing to subject")]
    PublishError,

    /// Failure to subscribe to the subject
    #[error("Error subscribing to subject")]
    SubscribeError,

    /// Errors that are not wrapped above
    #[error("Unknonwn error occurred")]
    Other,
}

/// Interface for a message queue
#[async_trait]
pub trait IMQClient<Message> {
    /// Open a connection to the message queue
    async fn connect(opts: Options) -> Self;

    /// Publish something to a particular subject
    async fn publish(&self, subject: &str, message: Vec<u8>) -> Result<()>;

    /// Subscribe to a subject
    async fn subscribe(&self, subject: &str) -> Result<Receiver<Message>>;

    /// Unsubscribe from a subject
    async fn unsubscrbe(&self, subject: &str) -> Result<()>;
}
