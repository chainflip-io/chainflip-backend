use async_trait::async_trait;
use futures::Stream;
use thiserror::Error;

// use super::nats_client::NatsReceiverAdapter;

/// Message should be deserialized by the individual components
#[derive(Debug, PartialEq, Clone)]
pub struct Message(pub Box<Vec<u8>>);

impl Message {
    pub fn new(data: Vec<u8>) -> Self {
        Message(Box::new(data))
    }
}

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

    /// Error when closing the connection
    #[error("Error closing the connection")]
    ErrorClosingConnection,

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
    async fn subscribe(
        &self,
        subject: &str,
    ) -> Result<Box<dyn Stream<Item = std::result::Result<Message, ()>>>>;

    /// Close the connection to the MQ
    async fn close(&self) -> Result<()>;
}
