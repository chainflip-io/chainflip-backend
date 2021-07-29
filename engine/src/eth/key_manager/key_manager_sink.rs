use crate::{
    eth::EventSink,
    mq::mq::{IMQClient, Subject},
};

use async_trait::async_trait;

use super::key_manager::KeyManagerEvent;

use anyhow::Result;

/// A sink that can be used with an EthEventStreamer instance
/// Pushes events to the message queue
pub struct KeyManagerSink<M: IMQClient + Send + Sync> {
    mq_client: M,
}

impl<M: IMQClient + Send + Sync> KeyManagerSink<M> {
    pub async fn new(mq_client: M) -> Result<KeyManagerSink<M>> {
        Ok(KeyManagerSink { mq_client })
    }
}

#[async_trait]
impl<M: IMQClient + Send + Sync> EventSink<KeyManagerEvent> for KeyManagerSink<M> {
    async fn process_event(&self, event: KeyManagerEvent) -> anyhow::Result<()> {
        log::debug!("Processing event in KeyManagerSink: {:?}", event);
        self.mq_client.publish(Subject::KeyManager, &event).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::{mq::nats_client::NatsMQClient, settings};

    use super::*;

    #[tokio::test]
    // Ensure it doesn't panic
    async fn create_key_manager_sink() {
        let server = nats_test_server::NatsTestServer::build().spawn();
        let addr = server.address();

        let ip = addr.ip();
        let port = addr.port();

        let mq_settings = settings::MessageQueue {
            endpoint: format!("http://{}:{}", ip, port),
        };

        let mq_client = NatsMQClient::new(&mq_settings).await.unwrap();

        KeyManagerSink::<NatsMQClient>::new(mq_client)
            .await
            .unwrap();
    }
}
