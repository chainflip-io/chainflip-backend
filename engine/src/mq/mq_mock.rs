#[cfg(test)]
extern crate nats_test_server;

use super::{nats_client::NatsMQClient, IMQClient, MQError, Message, Options};
use async_nats;
use async_stream::stream;
use async_trait::async_trait;
use tokio_stream::{Stream, StreamExt};

pub struct MockMQ {
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

impl MockMQ {
    /// Takes a nats test server instance and return a mock
    pub async fn new(server: nats_test_server::NatsTestServer) -> Self {
        let addr = server.address().to_string();
        let options = Options { url: addr };

        Self::connect(options).await
    }
}

#[async_trait]
impl IMQClient<Message> for MockMQ {
    async fn connect(opts: super::Options) -> Self {
        let conn = async_nats::connect(opts.url.as_str())
            .await
            .expect(&format!("Could not connect to Nats on {}", opts.url));
        MockMQ { conn }
    }

    async fn publish(&self, subject: super::Subject, message_data: Vec<u8>) -> super::Result<()> {
        self.conn
            .publish(&subject.to_string(), message_data)
            .await
            .map_err(|_| MQError::PublishError)
    }

    async fn subscribe(
        &self,
        subject: super::Subject,
    ) -> super::Result<Box<dyn futures::Stream<Item = Message>>> {
        let sub = self
            .conn
            .subscribe(&subject.to_string())
            .await
            .map_err(|_| MQError::SubscribeError)?;

        let subscription = Subscription { inner: sub };

        Ok(Box::new(subscription.into_stream()))
    }

    async fn close(&self) -> super::Result<()> {
        self.conn
            .close()
            .await
            .map_err(|_| MQError::ClosingConnectionError)
    }
}


// Ensure hte 
#[cfg(test)]
mod test {
    use super::*;

    
}