use super::{contracts::EventSource, EventProcessor, Result};
use futures::StreamExt;
use web3::types::BlockNumber;

pub struct EventStreamer<E: 'static + EventSource, P: EventProcessor<E>> {
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
    event_source: E,
    event_processor: P,
}

impl<E: 'static + EventSource, P: EventProcessor<E>> EventStreamer<E, P> {
    pub async fn new(url: &str, event_source: E, event_processor: P) -> Result<Self> {
        let transport = ::web3::transports::WebSocket::new(url).await?;

        Ok(Self {
            web3_client: ::web3::Web3::new(transport),
            event_source,
            event_processor,
        })
    }

    /// Create a stream of Ethereum log events.
    pub async fn run(&self, from_block: Option<u64>) -> Result<()> {
        let filter = self
            .event_source
            .filter_builder(
                from_block.map_or(BlockNumber::Pending, |h| BlockNumber::Number(h.into())),
            )
            .build();

        let event_stream = self
            .web3_client
            .eth_subscribe()
            .subscribe_logs(filter)
            .await?
            .map(|log_result| self.event_source.parse_event(log_result?));

        let processing_loop = event_stream.for_each_concurrent(None, |event| async {
            self.event_processor.process_event(event.unwrap()).await
        });

        Ok(processing_loop.await)
    }
}
