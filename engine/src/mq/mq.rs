use crossbeam_channel::Receiver;
use thiserror::Error;

/// Message should be deserialized by the individual components
#[derive(Debug, PartialEq, Clone)]
pub struct Message(pub Vec<u8>);

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

    /// Failure to convert channel between types
    #[error("Error converting channel to generic Message type")]
    ConversionError,

    /// Errors that are not wrapped above
    #[error("Unknonwn error occurred")]
    Other,
}

/// Message Queue Result type
pub type Result<T> = std::result::Result<T, MQError>;

/// Interface for a message queue
pub trait IMQClient<Message> {
    /// Open a connection to the message queue
    fn connect(opts: Options) -> Self;

    /// Publish something to a particular subject
    fn publish(&self, subject: &str, message: Vec<u8>) -> Result<()>;

    /// Subscribe to a subject
    fn subscribe(&self, subject: &str) -> Result<Receiver<Message>>;
}
