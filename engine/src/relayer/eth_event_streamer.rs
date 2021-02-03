use super::{EventSink, EventSource, Result};
use futures::StreamExt;
use tokio_compat_02::FutureExt;
use web3::types::BlockNumber;

pub struct EthEventStreamer<S: EventSource, P: EventSink<S::Event>> {
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
    event_source: S,
    event_sink: P,
}

impl<S: EventSource, P: EventSink<S::Event>> EthEventStreamer<S, P> {
    pub async fn new(url: &str, event_source: S, event_sink: P) -> Result<Self> {
        let transport = ::web3::transports::WebSocket::new(url).compat().await?;

        Ok(Self {
            web3_client: ::web3::Web3::new(transport),
            event_source,
            event_sink,
        })
    }

    /// Create a stream of Ethereum log events.
    pub async fn run(&self, from_block: Option<u64>) -> Result<()> {
        let filter = self
            .event_source
            .filter_builder(from_block.map_or(BlockNumber::Pending, |h| h.into()))
            .build();

        let log_stream = self
            .web3_client
            .eth_subscribe()
            .subscribe_logs(filter)
            .compat()
            .await?;

        let event_stream = log_stream.map(|log_result| self.event_source.parse_event(log_result?));

        let processing_loop = event_stream.for_each_concurrent(None, |parse_result| async {
            match parse_result {
                Ok(event) => self.event_sink.process_event(event).await,
                Err(e) => log::error!("Unable to parse event: {}.", e.backtrace()),
            }
        });

        log::info!("Subscribed. Listening for events.");

        processing_loop.await;

        log::info!("Subscription ended.");

        Ok(())
    }
}
