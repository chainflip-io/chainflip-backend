use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use crate::{mq::pin_message_stream, settings};

use super::IMQClient;
use anyhow::{Context, Result};
use async_trait::async_trait;
use log::*;
use parking_lot::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

pub struct MQMock2 {
    topics: Arc<Mutex<HashMap<String, Vec<UnboundedSender<String>>>>>,
}

pub struct MQMock2Client {
    topics: Arc<Mutex<HashMap<String, Vec<UnboundedSender<String>>>>>,
}

impl MQMock2 {
    pub fn new() -> Self {
        MQMock2 {
            topics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_client(&self) -> MQMock2Client {
        MQMock2Client {
            topics: Arc::clone(&self.topics),
        }
    }
}

#[async_trait]
impl IMQClient for MQMock2Client {
    async fn connect(_opts: settings::MessageQueue) -> Result<Box<Self>> {
        todo!();
    }

    async fn publish<M: 'static + serde::Serialize + Sync>(
        &self,
        subject: super::Subject,
        message: &'_ M,
    ) -> Result<()> {
        let subject = subject.to_string();

        match self.topics.lock().entry(subject) {
            Entry::Occupied(entry) => {
                let data = serde_json::to_string(message).unwrap();
                for sender in entry.get() {
                    sender.send(data.clone()).unwrap();
                }
                Ok(())
            }
            Entry::Vacant(_entry) => {
                // dropping message
                warn!("Dropping a message published into a topic with no subscribers");
                Ok(())
            }
        }
    }

    async fn subscribe<M: serde::de::DeserializeOwned>(
        &self,
        subject: super::Subject,
    ) -> Result<Box<dyn futures::Stream<Item = Result<M>>>> {
        let subject = subject.to_string();

        let mut topics = self.topics.lock();
        let entry = topics.entry(subject).or_default();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        entry.push(tx);

        let rx = UnboundedReceiverStream::new(rx).map(|x| serde_json::from_str(&x).context("subscribe"));

        return Ok(Box::new(rx));
    }

    async fn close(&self) -> Result<()> {
        todo!()
    }
}

#[tokio::test]
async fn test_own_mq() {
    let mq = MQMock2::new();

    let c1 = mq.get_client();
    let c2 = mq.get_client();
    let c3 = mq.get_client();

    let stream2 = c2.subscribe::<String>(super::Subject::P2PIncoming).await.unwrap();
    let mut stream2 = pin_message_stream(stream2);

    let stream3 = c3.subscribe::<String>(super::Subject::P2PIncoming).await.unwrap();
    let mut stream3 = pin_message_stream(stream3);

    let msg = "Test".to_string();

    c1.publish(super::Subject::P2PIncoming, &msg.clone()).await.unwrap();

    assert_eq!(stream2.next().await.unwrap().unwrap(), msg.clone());
    assert_eq!(stream3.next().await.unwrap().unwrap(), msg.clone());

}
