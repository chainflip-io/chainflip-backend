use super::{IMQClient, Options, Subject};
use anyhow::Context;
use async_nats;
use async_stream::stream;
use async_trait::async_trait;
use nats_test_server::NatsTestServer;
use tokio_stream::{Stream, StreamExt};

pub struct MockMQ {
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

/// # Using MockMQ
/// ```
/// let server = NatsTestServer::build().spawn();
/// let mock_mq = MockMQ::new(&server).await;
/// ```
impl MockMQ {
    pub async fn new(server: &NatsTestServer) -> Self {
        let addr = server.address().to_string();
        let options = Options { url: addr };

        *MockMQ::connect(options)
            .await
            .expect("Failed to initialise MockMQ")
    }
}

#[async_trait]
impl IMQClient for MockMQ {
    /// This should never really be called by testing functions, instead tests should use
    /// MockMQ::new()
    async fn connect(opts: Options) -> anyhow::Result<Box<Self>> {
        let conn = async_nats::connect(opts.url.as_str()).await?;
        Ok(Box::new(MockMQ { conn }))
    }

    async fn publish<M: 'static + serde::Serialize + Sync>(
        &self,
        subject: super::Subject,
        message: &'_ M,
    ) -> anyhow::Result<()> {
        let bytes = serde_json::to_string(message)?;
        let bytes = bytes.as_bytes();
        self.conn.publish(&subject.to_string(), bytes).await?;
        Ok(())
    }

    async fn subscribe<M: serde::de::DeserializeOwned>(
        &self,
        subject: super::Subject,
    ) -> anyhow::Result<Box<dyn Stream<Item = anyhow::Result<M>>>> {
        let sub = self.conn.subscribe(&subject.to_string()).await?;

        // NOTE: can we have more than one type of message on the same channel?
        // Should the messages of the wrong type be filtered out?

        let subscription = Subscription { inner: sub };
        let stream = subscription.into_stream().map(|bytes| {
            serde_json::from_slice(&bytes[..]).context("Message deserialization failed.")
        });

        Ok(Box::new(stream))
    }

    async fn close(&self) -> anyhow::Result<()> {
        let conn = self.conn.close().await?;
        Ok(conn)
    }
}

// Ensure the mock can do it's ting
mod test {
    use super::*;
    use crate::mq::pin_message_stream;
    use chainflip_common::types::coin::Coin;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct TestMessage(String);

    async fn subscribe_test_inner(mock_client: MockMQ) {
        let test_message = TestMessage(String::from("I SAW A TRANSACTION"));

        let subject = Subject::Witness(Coin::ETH);

        let stream = mock_client.subscribe::<TestMessage>(subject).await.unwrap();

        mock_client.publish(subject, &test_message).await.unwrap();

        let mut stream = pin_message_stream(stream);

        match tokio::time::timeout(std::time::Duration::from_millis(100), stream.next()).await {
            Ok(Some(m)) => assert_eq!(m.unwrap(), test_message),
            Ok(None) => panic!("Unexpected error: stream returned early."),
            Err(_) => panic!("Nats stream timed out."),
        };
    }

    // Use the nats test server instead of the running nats instance
    #[tokio::test]
    async fn subscribe_mock_mq() {
        let server = NatsTestServer::build().spawn();
        let mock_client = MockMQ::new(&server).await;

        subscribe_test_inner(mock_client).await;
    }
}
