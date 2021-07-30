use crate::{
    eth::EventSink,
    mq::mq::{IMQClient, Subject},
};

use async_trait::async_trait;

use super::key_manager::KeyManagerEvent;

use anyhow::Result;

/// A sink that can be used with an EthEventStreamer instance
/// Pushes events to the message queue
pub struct KeyManagerSink<MQC: IMQClient + Send + Sync> {
    mq_client: MQC,
}

impl<MQC: IMQClient + Send + Sync> KeyManagerSink<MQC> {
    pub async fn new(mq_client: MQC) -> Result<KeyManagerSink<MQC>> {
        Ok(KeyManagerSink { mq_client })
    }
}

#[async_trait]
impl<MQC: IMQClient + Send + Sync> EventSink<KeyManagerEvent> for KeyManagerSink<MQC> {
    async fn process_event(&self, event: KeyManagerEvent) -> Result<()> {
        log::debug!("Processing event in KeyManagerSink: {:?}", event);
        self.mq_client.publish(Subject::KeyManager, &event).await?;
        Ok(())
    }
}
