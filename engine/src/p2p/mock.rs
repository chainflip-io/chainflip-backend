use std::{collections::HashMap, sync::Arc};

use futures::Stream;
use parking_lot::Mutex;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::{P2PMessage, P2PNetworkClient, ValidatorId};

use async_trait::async_trait;

pub struct P2PClientMock {
    id: ValidatorId,
    pub receiver: Option<UnboundedReceiver<P2PMessage>>,
    network_inner: Arc<Mutex<NetworkMockInner>>,
}

impl P2PClientMock {
    pub fn new(id: ValidatorId, network_inner: Arc<Mutex<NetworkMockInner>>) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        network_inner.lock().register(&id, sender);

        P2PClientMock {
            id,
            receiver: Some(receiver),
            network_inner,
        }
    }
}

#[async_trait]
impl P2PNetworkClient for P2PClientMock {
    fn broadcast(&self, data: &[u8]) {
        self.network_inner.lock().broadcast(&self.id, data);
    }

    fn send(&self, to: &ValidatorId, data: &[u8]) {
        self.network_inner.lock().send(&self.id, to, data);
    }

    fn take_receiver(&mut self) -> Option<UnboundedReceiver<P2PMessage>> {
        self.receiver.take()
    }
}

pub struct NetworkMock(Arc<Mutex<NetworkMockInner>>);

impl NetworkMock {
    pub fn new() -> Self {
        let inner = NetworkMockInner::new();
        let inner = Arc::new(Mutex::new(inner));

        NetworkMock(inner)
    }

    pub fn new_client(&self, id: ValidatorId) -> P2PClientMock {
        P2PClientMock::new(id, Arc::clone(&self.0))
    }
}

pub struct NetworkMockInner {
    clients: HashMap<ValidatorId, UnboundedSender<P2PMessage>>,
}

impl NetworkMockInner {
    fn new() -> Self {
        NetworkMockInner {
            clients: HashMap::new(),
        }
    }

    /// Register validator, so we know how to contact them
    fn register(&mut self, id: &ValidatorId, sender: UnboundedSender<P2PMessage>) {
        let added = self.clients.insert(id.to_owned(), sender).is_none();
        assert!(added, "Cannot insert the same validator more than once");
    }

    fn broadcast(&self, from: &ValidatorId, data: &[u8]) {
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
    fn send(&self, from: &ValidatorId, to: &ValidatorId, data: &[u8]) {
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
