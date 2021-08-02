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
    async fn process_event(&self, event: StakeManagerEvent) -> Result<()> {
        slog::debug!(self.logger, "Processing event: {:?}", event);
        self.mq_client
            .publish(Subject::StakeManager, &event)
            .await?;
        Ok(())
    }
}
