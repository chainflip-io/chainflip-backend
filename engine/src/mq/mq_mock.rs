use std::{
    collections::{hash_map::Entry, HashMap},
    pin::Pin,
    sync::Arc,
};

use super::{IMQClient, SubjectName};
use anyhow::{Context, Result};
use async_trait::async_trait;
use log::*;
use parking_lot::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

/// In-memory message queue to be used in tests
#[derive(Clone)]
pub struct MQMock {
    topics: Arc<Mutex<HashMap<String, Vec<UnboundedSender<String>>>>>,
}

/// Client for MQMock
#[derive(Clone)]
pub struct MQMockClient {
    topics: Arc<Mutex<HashMap<String, Vec<UnboundedSender<String>>>>>,
}

impl MQMock {
    pub fn new() -> Self {
        MQMock {
            topics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_client(&self) -> MQMockClient {
        MQMockClient {
            topics: Arc::clone(&self.topics),
        }
    }
}

#[async_trait]
impl IMQClient for MQMockClient {
    async fn publish<M: 'static + serde::Serialize + Sync>(
        &self,
        subject: super::Subject,
        message: &'_ M,
    ) -> Result<()> {
        let subject = subject.to_subject_name();

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
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<M>>>>> {
        let subject = subject.to_subject_name();

        let mut topics = self.topics.lock();
        let entry = topics.entry(subject).or_default();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        entry.push(tx);

        let rx =
            UnboundedReceiverStream::new(rx).map(|x| serde_json::from_str(&x).context("subscribe"));

        return Ok(Box::pin(rx));
    }

    async fn close(&self) -> Result<()> {
        todo!()
    }
}

#[tokio::test]
async fn test_own_mq() {
    let mq = MQMock::new();

    let c1 = mq.get_client();
    let c2 = mq.get_client();
    let c3 = mq.get_client();

    let mut stream2 = c2
        .subscribe::<String>(super::Subject::P2PIncoming)
        .await
        .unwrap();

    let mut stream3 = c3
        .subscribe::<String>(super::Subject::P2PIncoming)
        .await
        .unwrap();

    let msg = "Test".to_string();

    c1.publish(super::Subject::P2PIncoming, &msg.clone())
        .await
        .unwrap();

    assert_eq!(stream2.next().await.unwrap().unwrap(), msg.clone());
    assert_eq!(stream3.next().await.unwrap().unwrap(), msg.clone());
}
