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
use jsonrpc_core_client::TypedSubscriptionStream;
use cf_p2p_rpc::P2pEvent;
use futures::stream::StreamExt;

#[derive(Debug)]
pub enum P2PNetworkClientError {
    Format,
    Rpc
}

type StatusCode = u64;

#[async_trait]
pub trait P2PNetworkClient<B: Base58> {
    /// Broadcast to all validators on the network
    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError>;

    /// Send to a specific `validator` only
    async fn send(&self, to: &B, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError>;

    async fn take_stream(&mut self) -> Result<TypedSubscriptionStream<P2pEvent>, P2PNetworkClientError>;
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

        clients[0].send(&1, &data);

        drop(network);

        let mut stream_1 =
            P2PNetworkClient::<ValidatorId>::take_stream(&mut clients[1]).await.unwrap();

        assert_eq!(
            stream_1.next().await.unwrap().unwrap(),
            P2pEvent::Received(0.to_string(), data.clone())
        );

        let mut stream_2 =
            P2PNetworkClient::<ValidatorId>::take_stream(&mut clients[2]).await.unwrap();

        assert!(stream_2.next().await.is_none());
    }

    #[tokio::test]
    async fn test_p2p_mock_broadcast() {
        let network = NetworkMock::new();

        let data = vec![3, 2, 1];
        let mut clients: Vec<_> = (0..3).map(|i| network.new_client(i)).collect();

        // (1) broadcasts; (0) and (2) should receive one message
        P2PNetworkClient::<ValidatorId>::broadcast(&clients[1], &data);

        let mut stream_0 =
            P2PNetworkClient::<ValidatorId>::take_stream(&mut clients[0]).await.unwrap();

        assert_eq!(
            stream_0.next().await.unwrap().unwrap(),
            P2pEvent::Received(1.to_string(), data.clone())
        );

        let mut stream_2 =
            P2PNetworkClient::<ValidatorId>::take_stream(&mut clients[2]).await.unwrap();

        assert_eq!(
            stream_2.next().await.unwrap().unwrap(),
            P2pEvent::Received(1.to_string(), data.clone())
        );
    }
}
