use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crossbeam_channel::{Receiver, Sender, TrySendError};

use super::{Message, P2PNetworkClient, ValidatorId};

pub(super) struct NetworkMock {
    id: ValidatorId,
    pub(super) receiver: Receiver<Message>,
    network_inner: Arc<Mutex<NetworkInner>>,
}

impl NetworkMock {
    pub fn new(id: ValidatorId, network_inner: Arc<Mutex<NetworkInner>>) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();

        network_inner.lock().unwrap().register(&id, sender);

        NetworkMock {
            id,
            receiver,
            network_inner,
        }
    }
}

impl P2PNetworkClient for NetworkMock {
    fn broadcast(&self, data: &[u8]) {
        self.network_inner.lock().unwrap().broadcast(&self.id, data);
    }

    fn send(&self, to: &ValidatorId, data: &[u8]) {
        self.network_inner.lock().unwrap().send(&self.id, to, data);
    }
}

pub struct NetworkInner {
    clients: HashMap<ValidatorId, Sender<Message>>,
}

impl NetworkInner {
    pub fn new() -> Self {
        NetworkInner {
            clients: HashMap::new(),
        }
    }

    /// Register validator, so we know how to contact them
    fn register(&mut self, id: &ValidatorId, sender: Sender<Message>) {
        let added = self.clients.insert(id.to_owned(), sender).is_none();
        assert!(added, "Cannot insert the same validator more than once");
    }

    fn broadcast(&self, from: &ValidatorId, data: &[u8]) {
        let m = Message {
            sender_id: from.to_owned(),
            data: data.to_owned(),
        };

        for (id, sender) in &self.clients {
            // Do not send to ourselves
            if id != from {
                match sender.try_send(m.clone()) {
                    Ok(()) => (),
                    Err(TrySendError::Full(_)) => {
                        panic!("channel is full");
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        panic!("channel is disconnected");
                    }
                }
            }
        }
    }

    /// Send to a specific `validator` only
    fn send(&self, from: &ValidatorId, to: &ValidatorId, data: &[u8]) {
        let m = Message {
            sender_id: from.to_owned(),
            data: data.to_owned(),
        };

        match self.clients.get(to).unwrap().try_send(m) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => {
                panic!("channel is full");
            }
            Err(TrySendError::Disconnected(_)) => {
                panic!("channel is disconnected");
            }
        }
    }
}
