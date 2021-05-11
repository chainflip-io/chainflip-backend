#[cfg(test)]
mod mock;

mod conductor;

use serde::{Deserialize, Serialize};

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedReceiver;

#[async_trait]
pub trait P2PNetworkClient {
    /// Broadcast to all validators on the network
    fn broadcast(&self, data: &[u8]);

    /// Send to a specific `validator` only
    fn send(&self, to: &ValidatorId, data: &[u8]);

    fn take_receiver(&mut self) -> Option<UnboundedReceiver<P2PMessage>>;
}

type ValidatorId = usize;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct P2PMessage {
    sender_id: ValidatorId,
    data: Vec<u8>,
}

/// A command to the conductor to send message `data` to
/// validator `destination`
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CommandSendMessage {
    destination: ValidatorId,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {

    use super::mock::*;
    use super::*;

    #[tokio::test]
    async fn test_p2p_mock_send() {
        let network = NetworkMock::new();

        let data = vec![1, 2, 3];
        let mut clients: Vec<_> = (0..3).map(|i| network.new_client(i)).collect();

        // (0) sends to (1); (1) should receive one, (2) receives none
        clients[0].send(&1, &data);

        drop(network);

        let mut receiver_1 = clients[1].take_receiver().unwrap();

        assert_eq!(
            receiver_1.recv().await.unwrap(),
            P2PMessage {
                sender_id: 0,
                data: data.clone()
            }
        );

        let mut receiver_2 = clients[1].take_receiver().unwrap();

        assert_eq!(receiver_2.recv().await, None);
    }

    #[tokio::test]
    async fn test_p2p_mock_broadcast() {
        let network = NetworkMock::new();

        let data = vec![3, 2, 1];
        let mut clients: Vec<_> = (0..3).map(|i| network.new_client(i)).collect();

        // (1) broadcasts; (0) and (2) should receive one message
        clients[1].broadcast(&data);
        let mut receiver_0 = clients[0].take_receiver().unwrap();
        assert_eq!(
            receiver_0.recv().await,
            Some(P2PMessage {
                sender_id: 1,
                data: data.clone()
            })
        );
        let mut receiver_2 = clients[2].take_receiver().unwrap();
        assert_eq!(
            receiver_2.recv().await,
            Some(P2PMessage {
                sender_id: 1,
                data: data.clone()
            })
        );
    }
}
