use super::{unsafe_pin_message_stream, IMQClient, MQError, Message, Options, Result};
use async_nats;
use async_stream::try_stream;
use async_trait::async_trait;
use nats;
use tokio_stream::{Stream, StreamExt};

// This will likely have a private field containing the underlying mq client
#[derive(Clone)]
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: async_nats::Connection,
}

impl From<nats::Message> for Message {
    fn from(m: nats::Message) -> Self {
        Message::new(m.data)
    }
}

pub struct Subscription {
    inner: async_nats::Subscription,
}

impl Subscription {
    pub fn into_stream(self) -> impl Stream<Item = std::result::Result<Message, ()>> {
        try_stream! {
            while let Some(m) = self.inner.next().await {
                yield Message::new(m.data);
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

    async fn publish(&self, subject: &str, message: Vec<u8>) -> Result<()> {
        self.conn
            .publish(subject, message)
            .await
            .map_err(|_| MQError::PublishError)
    }

    async fn subscribe(
        &self,
        subject: &str,
    ) -> Result<Box<dyn Stream<Item = std::result::Result<Message, ()>>>> {
        let sub = self
            .conn
            .subscribe(subject)
            .await
            .map_err(|_| MQError::SubscribeError)?;

        let subscription = Subscription { inner: sub };

        Ok(Box::new(subscription.into_stream()))
    }

    async fn close(&self) -> Result<()> {
        self.conn
            .close()
            .await
            .map_err(|_| MQError::ErrorClosingConnection)
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
            .publish("witness.eth", "hello".as_bytes().to_owned())
            .await;
        assert!(res.is_ok());
    }

    #[ignore = "Depends on Nats being online"]
    #[tokio::test]
    async fn subscribe_to_eth_witness() {
        let nats_client = setup_client().await;

        let test_message = "I SAW A TRANSACTION".as_bytes().to_owned();

        let stream = nats_client.subscribe("witness.eth").await.unwrap();

        nats_client
            .publish("witness.eth", test_message)
            .await
            .unwrap();

        let mut stream = unsafe_pin_message_stream(stream);

        tokio::spawn(async move {
            // may require a sleep in here, but nats is fast enough to work without one atm
            nats_client.close().await.unwrap();
        });

        let mut count: i32 = 0;
        while let Some(m) = stream.next().await {
            match m {
                Ok(_) => {
                    count += 1;
                }
                Err(_) => {
                    break;
                }
            }
        }

        assert_eq!(count, 1);
    }

    // Use the nats test server instead of the running nats instance
    #[tokio::test]
    async fn nats_test_server_connect() {
        let server = NatsTestServer::build().spawn();

        let addr = server.address().to_string();
        let options = Options { url: addr };

        let nats_client = NatsMQClient::connect(options).await;

        let test_message = "I SAW A TRANSACTION".as_bytes().to_owned();

        let stream = nats_client.subscribe("witness.eth").await.unwrap();

        nats_client
            .publish("witness.eth", test_message)
            .await
            .unwrap();

        let mut stream = unsafe_pin_message_stream(stream);

        tokio::spawn(async move {
            // may require a sleep in here, but nats is fast enough to work without one atm
            nats_client.close().await.unwrap();
        });

        let mut count: i32 = 0;
        while let Some(m) = stream.next().await {
            match m {
                Ok(_) => {
                    count += 1;
                }
                Err(_) => {
                    break;
                }
            }
        }

        assert_eq!(count, 1);
    }
}
