use std::{collections::HashMap, sync::Arc};

use futures::stream::BoxStream;
use std::sync::Mutex;

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use super::{AccountId, NetworkEventHandler, P2PMessage, P2PNetworkClient};

use crate::p2p::StatusCode;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub struct P2PClientMock {
    id: AccountId,
    pub receiver: Arc<Mutex<Option<UnboundedReceiverStream<P2PMessage>>>>,
    network_inner: Arc<Mutex<NetworkMockInner>>,
}

impl P2PClientMock {
    pub fn new(id: AccountId, network_inner: Arc<Mutex<NetworkMockInner>>) -> Self {
        let (sender, receiver) = unbounded_channel();

        network_inner.lock().unwrap().register(&id, sender);

        P2PClientMock {
            id,
            receiver: Arc::new(Mutex::new(Some(UnboundedReceiverStream::new(receiver)))),
            network_inner,
        }
    }
}

#[async_trait]
impl P2PNetworkClient for P2PClientMock {
    type NetworkEvent = P2PMessage;

    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode> {
        self.network_inner.lock().unwrap().broadcast(&self.id, data);
        Ok(200)
    }

    async fn send(&self, to: &AccountId, data: &[u8]) -> Result<StatusCode> {
        self.network_inner.lock().unwrap().send(&self.id, to, data);
        Ok(200)
    }

    async fn take_stream(&self) -> Result<BoxStream<Self::NetworkEvent>> {
        let stream = self
            .receiver
            .lock()
            .unwrap()
            .take()
            .ok_or(anyhow!("Subscription Error"))?;

        Ok(Box::pin(stream))
    }
}

pub struct MockChannelEventHandler(UnboundedSender<P2PMessage>);

impl MockChannelEventHandler {
    pub fn new() -> (Self, UnboundedReceiver<P2PMessage>) {
        let (s, r) = unbounded_channel();
        (Self(s), r)
    }
}

#[async_trait]
impl NetworkEventHandler<P2PClientMock> for MockChannelEventHandler {
    async fn handle_event(&self, event: P2PMessage) {
        self.0.send(event).unwrap()
    }
}

pub struct NetworkMock(Arc<Mutex<NetworkMockInner>>);

impl NetworkMock {
    pub fn new() -> Self {
        let inner = NetworkMockInner::new();
        let inner = Arc::new(Mutex::new(inner));

        NetworkMock(inner)
    }

    pub fn new_client(&self, id: AccountId) -> P2PClientMock {
        P2PClientMock::new(id, Arc::clone(&self.0))
    }
}

pub struct NetworkMockInner {
    clients: HashMap<AccountId, UnboundedSender<P2PMessage>>,
}

impl NetworkMockInner {
    fn new() -> Self {
        NetworkMockInner {
            clients: HashMap::new(),
        }
    }

    /// Register validator, so we know how to contact them
    fn register(&mut self, id: &AccountId, sender: UnboundedSender<P2PMessage>) {
        let added = self.clients.insert(id.to_owned(), sender).is_none();
        assert!(added, "Cannot insert the same validator more than once");
    }

    fn broadcast(&self, from: &AccountId, data: &[u8]) {
        let m = P2PMessage {
            sender_id: from.to_owned(),
            data: data.to_owned(),
        };

        for (id, sender) in &self.clients {
            // Do not send to ourselves
            if id != from {
                match sender.send(m.clone()) {
                    Ok(()) => (),
                    Err(_) => {
                        panic!("channel is disconnected");
                    }
                }
            }
        }
    }

    /// Send to a specific `validator` only
    fn send(&self, from: &AccountId, to: &AccountId, data: &[u8]) {
        let m = P2PMessage {
            sender_id: from.to_owned(),
            data: data.to_owned(),
        };

        match self.clients.get(to) {
            Some(client) => match client.send(m) {
                Ok(()) => {}
                Err(_) => {
                    panic!("channel is disconnected");
                }
            },
            None => {
                eprintln!("Client not connected: {}", to);
            }
        }
    }
}
