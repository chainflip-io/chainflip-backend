use crate::{
    eth::EventSink,
    mq::mq::{IMQClient, Subject},
};

use async_trait::async_trait;

use super::stake_manager::StakeManagerEvent;

use anyhow::Result;

/// A sink that can be used with an EthEventStreamer instance
/// Pushes events to the message queue
pub struct StakeManagerSink<MQC: IMQClient + Send + Sync> {
    mq_client: MQC,
}

impl<MQC: IMQClient + Send + Sync> StakeManagerSink<MQC> {
    pub async fn new(mq_client: MQC) -> Result<StakeManagerSink<MQC>> {
        Ok(StakeManagerSink { mq_client })
    }
}

#[async_trait]
impl<MQC: IMQClient + Send + Sync> EventSink<StakeManagerEvent> for StakeManagerSink<MQC> {
    async fn process_event(&self, event: StakeManagerEvent) -> Result<()> {
        log::debug!("Processing event in StakeManagerSink: {:?}", event);
        self.mq_client
            .publish(Subject::StakeManager, &event)
            .await?;
        Ok(())
    }
}
