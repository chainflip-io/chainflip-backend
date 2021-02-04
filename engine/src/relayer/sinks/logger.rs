use crate::relayer::EventSink;

/// A simple Logger that implements `EventSink`.
pub struct Logger {
    level: log::Level,
}

impl Logger {
    /// Create a new Logger with the desired log level.
    pub fn new(level: log::Level) -> Self {
        Self { level }
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new(log::Level::Debug)
    }
}

#[async_trait]
impl<E> EventSink<E> for Logger
where
    E: 'static + Send + std::fmt::Debug,
{
    async fn process_event(&self, event: E) {
        log::log!(self.level, "Received event: {:?}", event);
    }
}
