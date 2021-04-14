use crossbeam_channel::Receiver;

/// Message should be deserialized by the individual components
pub type Message = Vec<u8>;

/// Contains various general message queue options
pub struct Options {
    pub url: &'static str,
}

/// Message Queue Error type
pub enum MQError {
    /// Errors that are not wrapped above
    Other,
}

/// Message Queue Result type
pub type Result<T> = std::result::Result<T, MQError>;

/// Interface for a message queue
pub trait IMQClient<Message> {
    /// Open a connection to the message queue
    fn connect(opts: Options) -> Self;

    /// Publish something to a particular subject
    fn publish(&self, subject: &str, message: Vec<u8>);

    /// Subscribe to a subject
    fn subscribe(&self, subject: &str) -> Result<Receiver<Message>>;
}
