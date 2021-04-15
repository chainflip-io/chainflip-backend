use std::pin::Pin;

use super::{IMQClient, MQError, Message, Options, Result};
use async_nats;
use async_stream::{stream, AsyncStream};
use async_trait::async_trait;
use futures::Future;
use futures_core::stream::Stream;
use futures_util::stream::StreamExt;
use nats;
use tokio::sync::mpsc::{self, Receiver};

// This will likely have a private field containing the underlying mq client
#[derive(Clone)]
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: async_nats::Connection,
}

impl From<nats::Message> for Message {
    fn from(msg: nats::Message) -> Self {
        Message(msg.data)
    }
}

/// Restrict this T to being a valid message type
pub struct ReceiverAdapter<T> {
    pub receiver: Receiver<T>,
}

#[async_trait]
impl IMQClient<Message> for NatsMQClient {
    async fn connect(opts: Options) -> Self {
        let conn = async_nats::connect(opts.url)
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

    async fn subscribe(&self, subject: &str) -> Result<Receiver<Message>> {
        let subscription = self
            .conn
            .subscribe(subject)
            .await
            .map_err(|_| MQError::SubscribeError)?;

        let (tx, rx) = mpsc::channel::<Message>(300);

        tokio::spawn(async move {
            loop {
                while let Some(m) = subscription.next().await {
                    tx.send(Message(m.data)).await.unwrap();
                }
            }
        });

        Ok(rx)
    }

    async fn unsubscrbe(&self, subject: &str) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod test {

    use super::*;

    async fn setup_client() -> NatsMQClient {
        let options = Options {
            url: "http://localhost:4222",
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

        let mut stream = nats_client.subscribe("witness.eth").await.unwrap();

        nats_client
            .publish("witness.eth", test_message)
            .await
            .unwrap();

        while let Some(item) = stream.recv().await {
            println!("Here is the item: {:#?}", item);
        }

        tpol

        // Publish something to the mq so that we can read it
        // std::thread::spawn(move || {
        //     let pub_res = nats_client.publish("witness.eth", test_message.clone());
        //     assert!(pub_res.is_ok());
        // });

        // let msg_received = receiver.recv();
        // println!("Message received: {:#?}", msg_received);
        // assert!(msg_received.is_ok())
        // assert_eq!(msg_received.0, test_message);
    }
}
