use std::marker::PhantomData;

use super::{pin_message_stream, IMQClient, Options, Subject};
use async_nats;
use async_stream::stream;
use async_trait::async_trait;
use chainflip_common::types::coin::Coin;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_stream::{Stream, StreamExt};

type Result<T> = anyhow::Result<T>;

// This will likely have a private field containing the underlying mq client
#[derive(Clone)]
pub struct NatsMQClient<M>
where
    M: Serialize + DeserializeOwned + Send,
{
    /// The nats.rs Connection to the Nats server
    conn: async_nats::Connection,
    _marker: PhantomData<M>,
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

#[async_trait(?Send)]
impl<'a, M> IMQClient<M> for NatsMQClient<M>
where
    M: Serialize + DeserializeOwned + Send + 'static,
{
    async fn connect(opts: Options) -> Result<Box<Self>> {
        let conn = async_nats::connect(opts.url.as_str()).await?;
        Ok(Box::new(NatsMQClient {
            conn,
            _marker: PhantomData,
        }))
    }

    async fn publish(&self, subject: Subject, message_data: M) -> Result<()> {
        let bytes = serde_json::to_string(&message_data)?;
        let bytes = bytes.as_bytes();
        self.conn.publish(&subject.to_string(), bytes).await?;
        Ok(())
    }

    async fn subscribe(&self, subject: Subject) -> Result<Box<dyn Stream<Item = Vec<u8>>>> {
        let sub = self.conn.subscribe(&subject.to_string()).await?;

        let subscription = Subscription { inner: sub };

        Ok(Box::new(subscription.into_stream()))
    }

    async fn close(&self) -> Result<()> {
        let conn = self.conn.close().await?;
        Ok(conn)
    }
}

#[cfg(test)]
mod test {

    use nats_test_server::*;

    use super::*;

    async fn setup_client() -> Box<NatsMQClient<Vec<u8>>> {
        let options = Options {
            url: "http://localhost:4222".to_string(),
        };

        NatsMQClient::connect(options).await.unwrap()
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
            .publish(Subject::Witness(Coin::ETH), "hello".as_bytes().to_owned())
            .await;
        assert!(res.is_ok());
    }

    async fn subscribe_test_inner(nats_client: Box<NatsMQClient<Vec<u8>>>) {
        let test_message = "I SAW A TRANSACTION".as_bytes().to_owned();
        let expected_message = serde_json::to_string(&test_message).unwrap();
        let expected_bytes = expected_message.as_bytes();

        let subject = Subject::Witness(Coin::ETH);

        let stream = nats_client.subscribe(subject).await.unwrap();

        nats_client
            .publish(subject, test_message.clone())
            .await
            .unwrap();

        let mut stream = pin_message_stream(stream);

        let mut count: i32 = 0;
        while let Some(m) = stream.next().await {
            count += 1;
            assert_eq!(m, expected_bytes);
        }

        assert_eq!(count, 1);
    }

    #[ignore = "Depends on Nats being online"]
    #[tokio::test]
    async fn subscribe_to_eth_witness() {
        let nats_client = setup_client().await;

        subscribe_test_inner(nats_client).await;
    }

    // Use the nats test server instead of the running nats instance
    #[tokio::test]
    async fn nats_test_server_connect() {
        let server = NatsTestServer::build().spawn();

        let addr = server.address().to_string();
        let options = Options { url: addr };

        let nats_client = NatsMQClient::connect(options).await.unwrap();

        subscribe_test_inner(nats_client).await;
    }
}
