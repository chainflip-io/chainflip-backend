// Note: we temporary allow mock in non-test code
#[cfg(test)]
pub mod mock;

mod conductor;
mod rpc;

pub use conductor::P2PConductor;

use serde::{Deserialize, Serialize};

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedReceiver;
use crate::p2p::rpc::Base58;
use futures::Stream;

#[derive(Debug)]
pub enum P2PNetworkClientError {
    Format,
    Rpc
}

type StatusCode = u64;

#[async_trait]
pub trait P2PNetworkClient<B: Base58, S: Stream<Item=P2PMessage>> {
    /// Broadcast to all validators on the network
    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError>;

    /// Send to a specific `validator` only
    async fn send(&self, to: &B, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError>;

    async fn take_stream(&mut self) -> Result<S, P2PNetworkClientError> {
        Err(P2PNetworkClientError::Rpc)
    }
}

pub type ValidatorId = usize;

impl Base58 for ValidatorId {
    fn to_base58(&self) -> String {
        // TODO implementation
        "".to_string()
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct P2PMessage {
    pub sender_id: ValidatorId,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PMessageCommand {
    pub destination: ValidatorId,
    pub data: Vec<u8>,
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

    async fn receive_with_timeout<T>(mut receiver: UnboundedReceiver<T>) -> Option<T> {
        let fut = receiver.recv();
        tokio::time::timeout(std::time::Duration::from_millis(5), fut)
            .await
            .unwrap_or(None)
    }

    #[tokio::test]
    async fn test_p2p_mock_send() {
        let network = NetworkMock::new();

        let data = vec![1, 2, 3];
        let mut clients: Vec<_> = (0..3).map(|i| network.new_client(i)).collect();

        // (0) sends to (1); (1) should receive one, (2) receives none

        clients[0].send(&1, &data).await.unwrap();

        drop(network);

        let stream_1 = clients[1].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_1.into_inner()).await.unwrap(),
            P2PMessage {
                sender_id: 0,
                data: data.clone(),
            }
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(receive_with_timeout(stream_2.into_inner()).await, None);
    }

    #[tokio::test]
    async fn test_p2p_mock_broadcast() {
        let network = NetworkMock::new();

        let data = vec![3, 2, 1];
        let mut clients: Vec<_> = (0..3).map(|i| network.new_client(i)).collect();

        // (1) broadcasts; (0) and (2) should receive one message
        clients[1].broadcast(&data).await.unwrap();

        let stream_0 = clients[0].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_0.into_inner()).await.unwrap(),
            P2PMessage {
                sender_id: 1,
                data: data.clone(),
            }
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_2.into_inner()).await.unwrap(),
            P2PMessage {
                sender_id: 1,
                data: data.clone(),
            }
        );
    }
}
