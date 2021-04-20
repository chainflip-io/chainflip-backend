pub trait P2PNetwork {
    /// Broadcast to all validators on the network
    fn broadcast(&self, data: &[u8]);

    /// Send to a specific `validator` only
    fn send(&self, to: &ValidatorId, data: &[u8]);
}

type ValidatorId = usize;

#[derive(Clone, PartialEq, Debug)]
struct Message {
    sender_id: ValidatorId,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {

    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use crossbeam_channel::{Receiver, Sender, TrySendError};

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

    pub struct NetworkMock {
        id: ValidatorId,
        receiver: Receiver<Message>,
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

    impl P2PNetwork for NetworkMock {
        fn broadcast(&self, data: &[u8]) {
            self.network_inner.lock().unwrap().broadcast(&self.id, data);
        }

        fn send(&self, to: &ValidatorId, data: &[u8]) {
            self.network_inner.lock().unwrap().send(&self.id, to, data);
        }
    }

    use super::*;

    #[test]
    fn test_p2p_mock() {
        let inner_network = Arc::new(Mutex::new(NetworkInner::new()));

        let data = vec![1, 2, 3];
        let clients: Vec<_> = (0..3)
            .map(|i| NetworkMock::new(i, inner_network.clone()))
            .collect();

        // (0) sends to (1); (1) should receive one, (2) receives none
        clients[0].send(&1, &data);
        assert_eq!(
            clients[1].receiver.try_recv().unwrap(),
            Message {
                sender_id: 0,
                data: data.clone()
            }
        );
        assert!(clients[2].receiver.try_recv().is_err());

        let data = vec![3, 2, 1];

        // (1) broadcasts; (0) and (2) should receive one message
        clients[1].broadcast(&data);
        assert_eq!(
            clients[0].receiver.try_recv().unwrap(),
            Message {
                sender_id: 1,
                data: data.clone()
            }
        );
        assert_eq!(
            clients[2].receiver.try_recv().unwrap(),
            Message {
                sender_id: 1,
                data: data.clone()
            }
        );
    }
}
