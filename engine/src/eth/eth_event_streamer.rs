use super::{EventSink, EventSource, Result};
use async_std::task;
use futures::{future::join_all, stream, StreamExt};
use std::time::Duration;
use tokio_compat_02::FutureExt;
use web3::types::{BlockNumber, SyncState};

pub struct EthEventStreamer<S: EventSource> {
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
    event_source: S,
    event_sinks: Vec<Box<dyn EventSink<S::Event>>>,
}

pub struct EthEventStreamBuilder<S: EventSource> {
    url: String,
    event_source: S,
    event_sinks: Vec<Box<dyn EventSink<S::Event>>>,
}

impl<S: EventSource> EthEventStreamBuilder<S> {
    pub fn new(url: &str, event_source: S) -> Self {
        Self {
            url: url.into(),
            event_source,
            event_sinks: Vec::new(),
        }
    }

    pub fn with_sink<E: 'static + EventSink<S::Event>>(mut self, sink: E) -> Self {
        self.event_sinks.push(Box::new(sink));
        self
    }

    pub async fn build(self) -> Result<EthEventStreamer<S>> {
        if self.event_sinks.is_empty() {
            anyhow::bail!("Can't build a stream with no sink.")
        } else {
            // this is using tokio 0.2 :( , how to push it up to tokio1???
            let transport = ::web3::transports::WebSocket::new(self.url.as_str())
                // TODO: Remove this compat once the websocket dep uses tokio1
                .compat()
                .await?;

            Ok(EthEventStreamer {
                web3_client: ::web3::Web3::new(transport),
                event_source: self.event_source,
                event_sinks: self.event_sinks,
            })
        }
    }
}

impl<S: EventSource> EthEventStreamer<S> {
    /// Create a stream of Ethereum log events. If `from_block` is `None`, starts at the pending block.
    pub async fn run(&self, from_block: Option<u64>) -> Result<()> {
        // Make sure the eth node is fully synced
        // let mut sync_stream = self.web3_client.eth_subscribe().subscribe_syncing().await?;
        loop {
            match self.web3_client.eth().syncing().await? {
                SyncState::Syncing(info) => log::info!("Waiting for eth node to sync: {:?}", info),
                SyncState::NotSyncing => {
                    log::info!("Eth node is synced, subscribing to log events.");
                    // sync_stream.unsubscribe().await?;
                    break;
                }
            }
            task::sleep(Duration::from_secs(4)).await;
        }

        // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
        // request past block via http and prepend them to the stream manually.
        let past_logs = if let Some(b) = from_block {
            let http_filter = self.event_source.filter_builder(b.into()).build();

            self.web3_client.eth().logs(http_filter).await?
        } else {
            Vec::new()
        };

        // This is the filter for the subscription. Explicitly set it to start at the pending block
        // since this is what happens in most cases anyway.
        let ws_filter = self
            .event_source
            .filter_builder(BlockNumber::Pending)
            .build();

        let future_logs = self
            .web3_client
            .eth_subscribe()
            .subscribe_logs(ws_filter)
            .await?;

        let log_stream = stream::iter(past_logs)
            .map(|log| Ok(log))
            .chain(future_logs);

        let event_stream = log_stream.map(|log_result| self.event_source.parse_event(log_result?));

        let processing_loop = event_stream.for_each_concurrent(None, |parse_result| async {
            match parse_result {
                Ok(event) => {
                    join_all(self.event_sinks.iter().map(|sink| async move {
                        sink.process_event(event)
                            .await
                            .map_err(|e| log::error!("Error while processing event:\n{}", e))
                    }))
                    .await;
                }
                Err(e) => log::error!("Unable to parse event: {}.", e),
            }
        });

        log::info!("Subscribed. Listening for events.");

        processing_loop.await;

        log::info!("Subscription ended.");

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::eth::stake_manager::stake_manager::{StakeManager, StakingEvent};

    use super::*;

    use async_trait::async_trait;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    pub struct MySink {}

    #[async_trait]
    impl EventSink<StakingEvent> for MySink {
        async fn process_event(&self, event: StakingEvent) -> Result<()> {
            println!("Processing event: {:#?}", event);
            Ok(())
        }
    }

    #[tokio::test]
    async fn try_subscribe_to_events() {
        let stake_manager = StakeManager::load(CONTRACT_ADDRESS).unwrap();
        let my_sink = MySink {};
        let sm_event_stream = EthEventStreamBuilder::new("ws://localhost:8545", stake_manager);
        let sm_event_stream = sm_event_stream.with_sink(my_sink).build().await.unwrap();

        println!("Starting the eth event streamer");
        sm_event_stream.run(Some(0)).await.unwrap();

        println!("Done sm_event stream");
        // let
        //     .with_sink(my_sink)
        //     .build()
        //     .await
        //     .unwrap();
    }
}
