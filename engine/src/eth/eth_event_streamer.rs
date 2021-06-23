use super::{EventSink, EventSource, Result};
use futures::{future::join_all, stream, StreamExt};
use std::time::Duration;
use web3::types::{BlockNumber, SyncState};

/// Steams events from a particular ETH Source, such as a smart contract
/// into a particular event sink
/// For example, see stake_manager/mod.rs
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
            let transport = ::web3::transports::WebSocket::new(self.url.as_str()).await?;

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
        loop {
            match self.web3_client.eth().syncing().await? {
                SyncState::Syncing(info) => {
                    log::info!("Waiting for eth node to sync: {:?}", info);
                }
                SyncState::NotSyncing => {
                    log::info!("Eth node is synced, subscribing to log events.");
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(4)).await;
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

    use crate::{
        eth::stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink},
        mq::{
            nats_client::{NatsMQClient, NatsMQClientFactory},
            IMQClientFactory,
        },
        settings,
    };

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_stake_manager_events() {
        let stake_manager = StakeManager::load(CONTRACT_ADDRESS).unwrap();

        let mq_settings = settings::test_utils::new_test_settings()
            .unwrap()
            .message_queue;

        let factory = NatsMQClientFactory::new(&mq_settings);

        let mq_client = *factory.create().await.unwrap();
        // create the sink, which pushes events to the MQ
        let sm_sink = StakeManagerSink::<NatsMQClient>::new(mq_client)
            .await
            .unwrap();
        let sm_event_stream = EthEventStreamBuilder::new("ws://localhost:8545", stake_manager);
        let sm_event_stream = sm_event_stream.with_sink(sm_sink).build().await.unwrap();

        sm_event_stream.run(Some(0)).await.unwrap();
    }
}
