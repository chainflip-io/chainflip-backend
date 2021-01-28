mod contracts;
mod eth_event_streamer;

pub use contracts::stake_manager::StakeManager;
pub use eth_event_streamer::EventStreamer;

use thiserror::Error;

use self::contracts::EventSource;

/// The result type for this module.
pub type Result<T> = core::result::Result<T, RelayerError>;

/// The `Error` type for this module.
#[derive(Error, Debug)]
pub enum RelayerError {
    /// Wrapper for `web3` errors.
    #[error(transparent)]
    Web3Error(#[from] web3::Error),

    /// Wrapper for errors in the event producers.
    #[error(transparent)]
    EventProducerError(#[from] contracts::EventProducerError),
}

/// Implement this for the substrate client.
#[async_trait(?Send)]
pub trait EventProcessor<S>
where
    S: EventSource + 'static,
{
    async fn process_event(&self, event: S::Event);
}
