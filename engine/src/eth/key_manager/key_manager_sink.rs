use crate::{
    eth::EventSink,
    logging::COMPONENT_KEY,
    mq::mq::{IMQClient, Subject},
};

use async_trait::async_trait;
use slog::o;

use super::key_manager::KeyManagerEvent;

use anyhow::Result;

/// A sink that can be used with an EthEventStreamer instance
/// Pushes events to the message queue
pub struct KeyManagerSink<MQC: IMQClient + Send + Sync> {
    mq_client: MQC,
    logger: slog::Logger,
}

impl<MQC: IMQClient + Send + Sync> KeyManagerSink<MQC> {
    pub async fn new(mq_client: MQC, logger: &slog::Logger) -> Result<KeyManagerSink<MQC>> {
        Ok(KeyManagerSink {
            mq_client,
            logger: logger.new(o!(COMPONENT_KEY => "KeyManagerSink")),
        })
    }
}

#[async_trait]
impl<MQC: IMQClient + Send + Sync> EventSink<KeyManagerEvent> for KeyManagerSink<MQC> {
    async fn process_event(&self, event: KeyManagerEvent) -> Result<()> {
        slog::debug!(
            self.logger,
            "Processing event in KeyManagerSink: {:?}",
            event
        );
        self.mq_client.publish(Subject::KeyManager, &event).await?;
        Ok(())
    }
}
