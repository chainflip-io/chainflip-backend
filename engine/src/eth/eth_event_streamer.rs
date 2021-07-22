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
            log::info!("Creating WS transport");
            let transport = ::web3::transports::WebSocket::new(self.url.as_str()).await?;
            log::info!("Created WS transport");
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
    // TODO: Why is this an Option?
    pub async fn run(&self, from_block: Option<u64>) -> Result<()> {
        // Make sure the eth node is fully synced
        log::info!("Start syncing ETH node from block: {:?}", from_block);
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

            // TODO: Does this sleep do anything or are we just blocked until synced??
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

        log::info!("Eth_subscribe to future logs");
        let future_logs = self
            .web3_client
            .eth_subscribe()
            .subscribe_logs(ws_filter)
            .await?;

        let log_stream = stream::iter(past_logs)
            .map(|log| Ok(log))
            .chain(future_logs);

        let event_stream = log_stream.map(|log_result| self.event_source.parse_event(log_result?));

        log::info!("Create the processing loop future");
        let processing_loop_fut = event_stream.for_each_concurrent(None, |parse_result| async {
            match parse_result {
                Ok(event) => {
                    log::debug!("ETH event being processed: {:?}", event);
                    join_all(self.event_sinks.iter().map(|sink| {
                        let event = event.clone();
                        async move {
                            sink.process_event(event)
                                .await
                                .map_err(|e| log::error!("Error while processing event:\n{}", e))
                        }
                    }))
                    .await;
                }
                Err(e) => log::error!("Unable to parse event: {}.", e),
            }
        });

        log::info!("ETH event streamer listening for events...");

        processing_loop_fut.await;

        let err_msg = "ETH event streamer has stopped!";
        log::error!("{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
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

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_stake_manager_events() {
        init();
        let stake_manager = StakeManager::load(CONTRACT_ADDRESS).unwrap();

        let settings = settings::test_utils::new_test_settings().unwrap();

        let factory = NatsMQClientFactory::new(&settings.message_queue);

        let mq_client = *factory.create().await.expect("Could not connect to MQ");
        // create the sink, which pushes events to the MQ
        let sm_sink = StakeManagerSink::<NatsMQClient>::new(mq_client)
            .await
            .unwrap();
        let sm_event_stream =
            EthEventStreamBuilder::new(&settings.eth.node_endpoint, stake_manager);
        let sm_event_stream = sm_event_stream.with_sink(sm_sink).build().await.unwrap();

        sm_event_stream
            .run(settings.eth.from_block.into())
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "testing"]
    async fn setup_transport() {
        let h2 = tokio::spawn(async move {
            println!("Creating transport");
            let transport = ::web3::transports::WebSocket::new(
                "wss://rinkeby.infura.io/ws/v3/8225b8de4cc94062959f38e0781586d1",
            )
            .await
            .unwrap();
            println!("created transport");
        });

        let h1 = tokio::spawn(async move {
            loop {
                // std::thread::sleep(std::time::Duration::from_secs(5));
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                println!("Another 5 seconds gone");
                // let _ = futures_util::future::ok::<(), ()>(()).await;
            }
        });

        futures::join!(h1, h2);

        // tokio::spawn(async move {
        //     loop {
        //         std::thread::sleep(std::time::Duration::from_secs(5));
        //         println!("Nummer zwei: Another 5 seconds gone");
        //     }
        // });
    }

    // #[tokio::test]
    // async fn setup_builder() {
    //     let sm_event_stream = EthEventStreamBuilder::new(
    //         "wss://rinkeby.infura.io/ws/v3/8225b8de4cc94062959f38e0781586d1",
    //         stake_manager,
    //     );
    //     sm_event_
    // }
}
