use super::{pin_message_stream, IMQClient, MQError, Message, Options, Result, Subject};
use async_nats;
use async_stream::stream;
use async_trait::async_trait;
use chainflip_common::types::coin::Coin;
use tokio_stream::{Stream, StreamExt};

// This will likely have a private field containing the underlying mq client
#[derive(Clone)]
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: async_nats::Connection,
}

struct Subscription {
    inner: async_nats::Subscription,
}

impl Subscription {
    pub fn into_stream(self) -> impl Stream<Item = Message> {
        stream! {
            while let Some(m) = self.inner.next().await {
                yield Message(m.data);
            }
        }
    }
}

#[async_trait]
impl IMQClient<Message> for NatsMQClient {
    async fn connect(opts: Options) -> Self {
        let conn = async_nats::connect(opts.url.as_str())
            .await
            .expect(&format!("Could not connect to Nats on {}", opts.url));
        NatsMQClient { conn }
    }

    async fn publish(&self, subject: Subject, message_data: Vec<u8>) -> Result<()> {
        self.conn
            .publish(&subject.to_string(), message_data)
            .await
            .map_err(|_| MQError::PublishError)
    }

    async fn subscribe(&self, subject: Subject) -> Result<Box<dyn Stream<Item = Message>>> {
        let sub = self
            .conn
            .subscribe(&subject.to_string())
            .await
            .map_err(|_| MQError::SubscribeError)?;

        let subscription = Subscription { inner: sub };

        Ok(Box::new(subscription.into_stream()))
    }

    async fn close(&self) -> Result<()> {
        self.conn
            .close()
            .await
            .map_err(|_| MQError::ClosingConnectionError)
    }
}

#[cfg(test)]
mod test {

    use nats_test_server::*;

    use super::*;

    async fn setup_client() -> NatsMQClient {
        let options = Options {
            url: "http://localhost:4222".to_string(),
        };

        NatsMQClient::connect(options).await
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

    async fn subscribe_test_inner(nats_client: NatsMQClient) {
        let test_message = "I SAW A TRANSACTION".as_bytes().to_owned();

        let subject = Subject::Witness(Coin::ETH);

        let stream = nats_client.subscribe(subject).await.unwrap();

        nats_client
            .publish(subject, test_message.clone())
            .await
            .unwrap();

        let mut stream = pin_message_stream(stream);

        tokio::spawn(async move {
            // may require a sleep in here, but nats is fast enough to work without one atm
            nats_client.close().await.unwrap();
        });

        let mut count: i32 = 0;
        while let Some(m) = stream.next().await {
            count += 1;
            assert_eq!(m.0, test_message);
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

        let nats_client = NatsMQClient::connect(options).await;

        subscribe_test_inner(nats_client).await;
    }
}
