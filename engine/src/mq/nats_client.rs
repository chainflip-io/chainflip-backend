use std::pin::Pin;

use super::SubjectName;
use super::{IMQClient, Subject};
use anyhow::Context;
use anyhow::Result;
use async_nats;
use async_stream::stream;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use tokio_stream::{Stream, StreamExt};

use crate::settings;

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

// This will likely have a private field containing the underlying mq client
#[derive(Clone, Debug)]
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: async_nats::Connection,
}

impl NatsMQClient {
    pub async fn new(mq_settings: &settings::MessageQueue) -> Result<Self> {
        let conn = async_nats::connect(mq_settings.endpoint.as_str()).await?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl IMQClient for NatsMQClient {
    async fn publish<M: Serialize + Sync>(&self, subject: Subject, message: &'_ M) -> Result<()> {
        let bytes = serde_json::to_string(message)?;
        let bytes = bytes.as_bytes();
        self.conn.publish(&subject.to_subject_name(), bytes).await?;
        Ok(())
    }

    async fn subscribe<M: DeserializeOwned>(
        &self,
        subject: Subject,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<M>>>>> {
        let sub = self.conn.subscribe(&subject.to_subject_name()).await?;

        let subscription = Subscription { inner: sub };
        let stream = subscription.into_stream().map(|bytes| {
            serde_json::from_slice(&bytes[..]).context("Message deserialization failed.")
        });

        Ok(Box::pin(stream))
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

    use crate::types::chain::Chain;
    use serde::Deserialize;

    use crate::mq::pin_message_stream;

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct TestMessage(String);

    async fn setup_client() -> NatsMQClient {
        let settings = settings::test_utils::new_test_settings().unwrap();
        NatsMQClient::new(&settings.message_queue).await.unwrap()
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
                Subject::Witness(Chain::ETH),
                &TestMessage(String::from("hello")),
            )
            .await;
        assert!(res.is_ok());
    }

    async fn subscribe_test_inner(nats_client: NatsMQClient) {
        let test_message = TestMessage(String::from("I SAW A TRANSACTION"));

        let subject = Subject::Witness(Chain::ETH);

        let mut test_messages = nats_client.subscribe::<TestMessage>(subject).await.unwrap();

        nats_client.publish(subject, &test_message).await.unwrap();

        match tokio::time::timeout(Duration::from_millis(100), test_messages.next()).await {
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
