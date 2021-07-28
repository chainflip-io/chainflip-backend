use crate::{
    eth::EventSink,
    logging::COMPONENT_KEY,
    mq::mq::{IMQClient, Subject},
};

use async_trait::async_trait;
use slog::o;

use super::stake_manager::StakeManagerEvent;

use anyhow::Result;

/// A sink that can be used with an EthEventStreamer instance
/// Pushes events to the message queue
pub struct StakeManagerSink<M: IMQClient + Send + Sync> {
    mq_client: M,
    logger: slog::Logger,
}

impl<M: IMQClient + Send + Sync> StakeManagerSink<M> {
    pub async fn new(mq_client: M, logger: &slog::Logger) -> Result<StakeManagerSink<M>> {
        Ok(StakeManagerSink {
            mq_client,
            logger: logger.new(o!(COMPONENT_KEY => "stake-manager-sink")),
        })
    }
}

#[async_trait]
impl<MQC: IMQClient + Send + Sync> EventSink<StakeManagerEvent> for StakeManagerSink<MQC> {
    async fn process_event(&self, event: StakeManagerEvent) -> anyhow::Result<()> {
        slog::debug!(self.logger, "Processing event: {:?}", event);
        self.mq_client
            .publish(Subject::StakeManager, &event)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::{logging, mq::nats_client::NatsMQClient, settings};

    use super::*;

    #[tokio::test]
    // Ensure it doesn't panic
    async fn create_stake_manager_sink() {
        let server = nats_test_server::NatsTestServer::build().spawn();
        let addr = server.address();
        let logger = logging::test_utils::create_test_logger();

        let mq_settings = settings::MessageQueue {
            endpoint: format!("http://{}:{}", addr.ip(), addr.port()),
        };

        let mq_client = NatsMQClient::new(&mq_settings).await.unwrap();

        StakeManagerSink::<NatsMQClient>::new(mq_client, &logger)
            .await
            .unwrap();
    }
}
