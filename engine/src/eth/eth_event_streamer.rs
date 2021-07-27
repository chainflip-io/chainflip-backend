use crate::logging::COMPONENT_KEY;

use super::{EventSink, EventSource, Result};
use futures::{future::join_all, stream, StreamExt};
use slog::o;
use std::time::Duration;
use web3::types::{BlockNumber, SyncState};

/// Steams events from a particular ETH Source, such as a smart contract
/// into a particular event sink
/// For example, see stake_manager/mod.rs
pub struct EthEventStreamer<S: EventSource> {
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
    event_source: S,
    event_sinks: Vec<Box<dyn EventSink<S::Event>>>,
    logger: slog::Logger,
}

pub struct EthEventStreamBuilder<'a, S: EventSource> {
    url: String,
    event_source: S,
    event_sinks: Vec<Box<dyn EventSink<S::Event>>>,
    logger: &'a slog::Logger,
}

impl<'a, S: EventSource> EthEventStreamBuilder<'a, S> {
    pub fn new(url: &str, event_source: S, logger: &'a slog::Logger) -> Self {
        Self {
            url: url.into(),
            event_source,
            event_sinks: Vec::new(),
            logger,
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
            let transport = ::web3::transports::WebSocket::new(self.url.as_str());

            let transport = match tokio::time::timeout(Duration::from_secs(4), transport).await {
                Ok(Ok(transport)) => transport,
                Err(_) => {
                    return Err(anyhow::Error::msg(format!(
                        "Timeout creating websocket to {} for EthEventStreamer",
                        self.url,
                    )));
                }
                _ => {
                    return Err(anyhow::Error::msg(format!(
                        "Failed to create websocket to {} for EthEventStreamer",
                        self.url,
                    )));
                }
            };

            Ok(EthEventStreamer {
                web3_client: ::web3::Web3::new(transport),
                event_source: self.event_source,
                event_sinks: self.event_sinks,
                logger: self.logger.new(o!(COMPONENT_KEY => "EthEventStreamer")),
            })
        }
    }
}

impl<S: EventSource> EthEventStreamer<S> {
    /// Create a stream of Ethereum log events. If `from_block` is `None`, starts at the pending block.
    pub async fn run(&self, from_block: Option<u64>) -> Result<()> {
        slog::info!(
            self.logger,
            "Start running eth event stream from block: {:?}",
            from_block
        );
        // Make sure the eth node is fully synced
        loop {
            match self.web3_client.eth().syncing().await? {
                SyncState::Syncing(info) => {
                    slog::info!(self.logger, "Waiting for eth node to sync: {:?}", info);
                }
                SyncState::NotSyncing => {
                    slog::info!(
                        self.logger,
                        "Eth node is synced, subscribing to log events."
                    );
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

        let processing_loop_fut = event_stream.for_each_concurrent(None, |parse_result| async {
            match parse_result {
                Ok(event) => {
                    join_all(self.event_sinks.iter().map(|sink| {
                        let event = event.clone();
                        async move {
                            sink.process_event(event).await.map_err(|e| {
                                slog::error!(self.logger, "Error while processing event:\n{}", e)
                            })
                        }
                    }))
                    .await;
                }
                Err(e) => slog::error!(self.logger, "Unable to parse event: {}.", e),
            }
        });

        slog::info!(self.logger, "Listening for events...");

        processing_loop_fut.await;

        let err_msg = "Stopped!";
        slog::error!(self.logger, "{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        eth::stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink},
        logging,
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
        let logger = logging::test_utils::create_test_logger();
        let stake_manager = StakeManager::load(CONTRACT_ADDRESS, &logger).unwrap();

        let mq_settings = settings::test_utils::new_test_settings()
            .unwrap()
            .message_queue;

        let factory = NatsMQClientFactory::new(&mq_settings);

        let mq_client = *factory.create().await.unwrap();

        // create the sink, which pushes events to the MQ
        let sm_sink = StakeManagerSink::<NatsMQClient>::new(mq_client, &logger)
            .await
            .unwrap();
        let sm_event_stream =
            EthEventStreamBuilder::new("ws://localhost:8545", stake_manager, &logger);
        let sm_event_stream = sm_event_stream.with_sink(sm_sink).build().await.unwrap();

        sm_event_stream.run(Some(0)).await.unwrap();
    }
}
