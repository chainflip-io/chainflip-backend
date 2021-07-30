use std::{collections::HashMap, sync::Arc};

use cf_p2p::{RawMessage, ValidatorId};
use parking_lot::Mutex;

use tokio::sync::mpsc::UnboundedSender;

use super::{P2PMessage, P2PNetworkClient};

use crate::p2p::{P2PNetworkClientError, StatusCode};
use async_trait::async_trait;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub struct P2PClientMock {
    id: ValidatorId,
    pub receiver: Option<UnboundedReceiverStream<P2PMessage>>,
    network_inner: Arc<Mutex<NetworkMockInner>>,
}

impl P2PClientMock {
    pub fn new(id: ValidatorId, network_inner: Arc<Mutex<NetworkMockInner>>) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        network_inner.lock().register(&id, sender);

        P2PClientMock {
            id,
            receiver: Some(UnboundedReceiverStream::new(receiver)),
            network_inner,
        }
    }
}

#[async_trait]
impl P2PNetworkClient<UnboundedReceiverStream<P2PMessage>> for P2PClientMock {
    async fn broadcast(&self, data: &RawMessage) -> Result<StatusCode, P2PNetworkClientError> {
        self.network_inner.lock().broadcast(&self.id, data);
        Ok(200)
    }

    async fn send(
        &self,
        to: &ValidatorId,
        data: &RawMessage,
    ) -> Result<StatusCode, P2PNetworkClientError> {
        self.network_inner.lock().send(&self.id, to, data);
        Ok(200)
    }

    async fn take_stream(
        &mut self,
    ) -> Result<UnboundedReceiverStream<P2PMessage>, P2PNetworkClientError> {
        self.receiver.take().ok_or(P2PNetworkClientError::Rpc)
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

    fn broadcast(&self, from: &ValidatorId, data: &RawMessage) {
        let m = P2PMessage {
            sender_id: from.to_owned(),
            data: data.0.clone(),
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
    fn send(&self, from: &ValidatorId, to: &ValidatorId, data: &RawMessage) {
        let m = P2PMessage {
            sender_id: from.to_owned(),
            data: data.0.clone(),
        };

        match self.clients.get(to) {
            Some(client) => match client.send(m) {
                Ok(()) => {}
                Err(_) => {
                    panic!("channel is disconnected");
                }
            },
            None => {
                eprintln!("Client not connected: {:?}", to);
            }
        }
    }
}
