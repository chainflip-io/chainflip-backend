use super::IMQClientFactory;
use super::{IMQClient, Subject};
use anyhow::Context;
use anyhow::Result;
use async_nats;
use async_stream::stream;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use tokio_stream::{Stream, StreamExt};

use crate::settings;

// This will likely have a private field containing the underlying mq client
#[derive(Clone, Debug)]
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: async_nats::Connection,
}

struct Subscription {
    inner: async_nats::Subscription,
}

impl Subscription {
    pub fn into_stream(self) -> impl Stream<Item = Vec<u8>> {
        stream! {
            while let Some(m) = self.inner.next().await {
                yield m.data;
            }
        }
    }
}

pub struct NatsMQClientFactory {
    mq_settings: settings::MessageQueue,
}

impl NatsMQClientFactory {
    pub fn new(mq_settings: &settings::MessageQueue) -> Self {
        NatsMQClientFactory {
            mq_settings: mq_settings.clone(),
        }
    }
}

#[async_trait]
impl IMQClientFactory<NatsMQClient> for NatsMQClientFactory {
    async fn create(&self) -> anyhow::Result<Box<NatsMQClient>> {
        let url = format!(
            "http://{}:{}",
            self.mq_settings.hostname, self.mq_settings.port
        );
        let conn = async_nats::connect(url.as_str()).await?;
        Ok(Box::new(NatsMQClient { conn }))
    }
}

#[async_trait]
impl IMQClient for NatsMQClient {
    async fn publish<M: Serialize + Sync>(&self, subject: Subject, message: &'_ M) -> Result<()> {
        let bytes = serde_json::to_string(message)?;
        let bytes = bytes.as_bytes();
        self.conn.publish(&subject.to_string(), bytes).await?;
        Ok(())
    }

    async fn subscribe<M: DeserializeOwned>(
        &self,
        subject: Subject,
    ) -> Result<Box<dyn Stream<Item = Result<M>>>> {
        let sub = self.conn.subscribe(&subject.to_string()).await?;

        let subscription = Subscription { inner: sub };
        let stream = subscription.into_stream().map(|bytes| {
            serde_json::from_slice(&bytes[..]).context("Message deserialization failed.")
        });

        Ok(Box::new(stream))
    }

    async fn close(&self) -> Result<()> {
        let conn = self.conn.close().await?;
        Ok(conn)
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use core::panic;
    use std::time::Duration;

    use chainflip_common::types::coin::Coin;
    use serde::Deserialize;

    use crate::mq::pin_message_stream;

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct TestMessage(String);

    async fn setup_client() -> Box<NatsMQClient> {
        let mq_settings = settings::MessageQueue {
            hostname: "localhost".to_string(),
            port: 4222,
        };

        NatsMQClientFactory::new(&mq_settings)
            .create()
            .await
            .unwrap()
    }

    #[ignore = "Depends on Nats being online"]
    #[tokio::test]
    async fn connect_to_nats() {
        let nats_client = setup_client().await;
        let client_ip = nats_client.conn.client_ip();
        assert!(client_ip.is_ok())
    }

    #[ignore = "Depends on Nats being online"]
    #[tokio::test]
    async fn publish_to_subject() {
        let nats_client = setup_client().await;
        let res = nats_client
            .publish(
                Subject::Witness(Coin::ETH),
                &TestMessage(String::from("hello")),
            )
            .await;
        assert!(res.is_ok());
    }

    async fn subscribe_test_inner(nats_client: Box<NatsMQClient>) {
        let test_message = TestMessage(String::from("I SAW A TRANSACTION"));

        let subject = Subject::Witness(Coin::ETH);

        let stream = nats_client.subscribe::<TestMessage>(subject).await.unwrap();

        nats_client.publish(subject, &test_message).await.unwrap();

        let mut stream = pin_message_stream(stream);

        match tokio::time::timeout(Duration::from_millis(100), stream.next()).await {
            Ok(Some(m)) => assert_eq!(m.unwrap(), test_message),
            Ok(None) => panic!("Unexpected error: stream returned early."),
            Err(_) => panic!("Nats stream timed out."),
        };
    }

    #[ignore = "Depends on Nats being online"]
    #[tokio::test]
    async fn subscribe_to_eth_witness() {
        let nats_client = setup_client().await;

        subscribe_test_inner(nats_client).await;
    }
}
