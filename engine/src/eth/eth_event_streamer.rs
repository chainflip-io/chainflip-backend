use crate::logging::COMPONENT_KEY;

use super::{EventSink, EventSource};
use futures::{future::join_all, stream, StreamExt};
use slog::o;
use web3::{
    types::BlockNumber,
    Web3,
    Transport,
    DuplexTransport
};

use anyhow::Result;

/// Steams events from a particular ETH Source, such as a smart contract
/// into a particular event sink
/// For example, see stake_manager/mod.rs
pub struct EthEventStreamer<E, S, T>
where
    E: EventSink<S::Event> + 'static,
    S: EventSource,
    T: Transport
{
    web3: Web3<T>,
    event_source: S,
    event_sinks: Vec<E>,
    logger: slog::Logger,
}

impl<S, E, T> EthEventStreamer<E, S, T>
where
    S: EventSource,
    E: EventSink<S::Event> + 'static,
    T : DuplexTransport
{
    /// Connects to the node_endpoint WebSocket with a 5sec timeout
    pub async fn new(
        web3 : &Web3<T>,
        event_source: S,
        event_sinks: Vec<E>,
        logger: &slog::Logger,
    ) -> Self {
        Self {
            web3 : web3.clone(),
            event_source,
            event_sinks,
            logger : logger.new(o!(COMPONENT_KEY => "EthEventStreamer"))
        }
    }

    /// Create a stream of Ethereum log events. If `from_block` is `None`, starts at the pending block.
    pub async fn run(&self, from_block: Option<u64>) -> Result<()> {
        slog::info!(
            self.logger,
            "Start running eth event stream from block: {:?}",
            from_block
        );

        // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
        // request past block via http and prepend them to the stream manually.
        let past_logs = if let Some(b) = from_block {
            self.web3
                .eth()
                .logs(self.event_source.filter_builder(b.into()).build())
                .await?
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
            .web3
            .eth_subscribe()
            .subscribe_logs(ws_filter)
            .await?;

        let log_stream = stream::iter(past_logs)
            .map(|log| Ok(log))
            .chain(future_logs);

        let event_stream = log_stream.map(|log_result| {
            self.event_source.parse_event(log_result?)}
        );

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
        eth::{new_web3_client, stake_manager::{stake_manager::StakeManager, stake_manager_sink::StakeManagerSink}},
        logging,
        mq::nats_client::NatsMQClient,
        settings,
    };

    use super::*;

    const CONTRACT_ADDRESS: &'static str = "0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f";

    #[tokio::test]
    #[ignore = "Depends on a running ganache instance, runs forever, useful for manually testing / observing incoming events"]
    async fn subscribe_to_stake_manager_events() {
        let logger = logging::test_utils::create_test_logger();

        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_client = NatsMQClient::new(&settings.message_queue).await.unwrap();

        EthEventStreamer::new(
            &new_web3_client(&settings, &logger).await.unwrap(),
            StakeManager::load(CONTRACT_ADDRESS).unwrap(),
            vec![StakeManagerSink::<NatsMQClient>::new(mq_client, &logger)
                .await
                .unwrap()],
            &logger,
        )
        .await
        .run(Some(0))
        .await
        .unwrap()
    }
}
