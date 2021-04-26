#[cfg(test)]
mod mock;

pub trait P2PNetworkClient {
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

    use std::sync::{Arc, Mutex};

    use super::mock::*;
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
