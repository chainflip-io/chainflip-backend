pub mod conductor;
#[cfg(test)]
pub mod mock;
pub mod rpc;

pub use cf_p2p::{P2PEvent, P2PRpcClient};

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

type StatusCode = u64;

/// Trait for handling messages to/from the P2P layer
/// i.e. messages that come from / go to *other* nodes in the network
#[async_trait]
pub trait P2PNetworkClient {
    type NetworkEvent;

    /// Broadcast to all validators on the network
    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode>;

    /// Send to a specific `validator` only
    async fn send(&self, to: &AccountId, data: &[u8]) -> Result<StatusCode>;

    /// Get a stream of notifications from the network.
    async fn take_stream(&self) -> Result<BoxStream<Self::NetworkEvent>>;
}

/// Handles P2P network events.
#[async_trait]
pub trait NetworkEventHandler<C: P2PNetworkClient + Send> {
    async fn handle_event(&self, event: C::NetworkEvent);
}

struct P2PRpcEventHandler {
    p2p_message_sender: UnboundedSender<P2PMessage>,
    logger: slog::Logger,
}

#[async_trait]
impl NetworkEventHandler<P2PRpcClient> for P2PRpcEventHandler {
    async fn handle_event(&self, network_event: Result<P2PEvent>) {
        match network_event {
            Ok(event) => match event {
                P2PEvent::MessageReceived(sender, message) => {
                    self.p2p_message_sender
                        .send(P2PMessage {
                            sender_id: AccountId(sender.0),
                            data: message.0,
                        })
                        .map_err(|_| "Receiver dropped")
                        .unwrap();
                }
                P2PEvent::ValidatorConnected(id) => {
                    slog::debug!(self.logger, "Validator '{}' has joined the network.", id);
                }
                P2PEvent::ValidatorDisconnected(id) => {
                    slog::debug!(self.logger, "Validator '{}' has left the network.", id);
                }
            },
            Err(e) => panic!("Subscription stream error: {}", e),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Eq, PartialOrd, Ord, Hash)]
pub struct AccountId(pub [u8; 32]);

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AccountId({})", bs58::encode(&self.0).into_string())
    }
}

impl std::fmt::Debug for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct P2PMessage {
    pub sender_id: AccountId,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PMessageCommand {
    pub destination: AccountId,
    pub data: Vec<u8>,
}

impl P2PMessageCommand {
    pub fn new(destination: AccountId, data: Vec<u8>) -> Self {
        P2PMessageCommand { destination, data }
    }
}

/// A command to the conductor to send message `data` to
/// validator `destination`
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CommandSendMessage {
    destination: AccountId,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use itertools::Itertools;

    use super::mock::*;
    use super::*;

    async fn receive_with_timeout<T>(mut stream: BoxStream<'_, T>) -> Option<T> {
        let fut = stream.next();
        tokio::time::timeout(std::time::Duration::from_millis(5), fut)
            .await
            .unwrap_or(None)
    }

    #[tokio::test]
    async fn test_p2p_mock_send() {
        let network = NetworkMock::new();

        let data = vec![1, 2, 3];
        let validator_ids = (0..3).map(|i| AccountId([i; 32])).collect_vec();

        let clients = validator_ids
            .iter()
            .map(|id| network.new_client(id.clone()))
            .collect_vec();

        // (0) sends to (1); (1) should receive one, (2) receives none
        clients[0].send(&validator_ids[1], &data).await.unwrap();

        drop(network);

        let stream_1 = clients[1].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_1).await,
            Some(P2PMessage {
                sender_id: validator_ids[0].clone(),
                data: data.clone()
            })
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(receive_with_timeout(stream_2).await, None);
    }

    #[tokio::test]
    async fn test_p2p_mock_broadcast() {
        let network = NetworkMock::new();

        let data = vec![3, 2, 1];
        let validator_ids = (0..3).map(|i| AccountId([i; 32])).collect_vec();
        let clients = validator_ids
            .iter()
            .map(|id| network.new_client(id.clone()))
            .collect_vec();

        // (1) broadcasts; (0) and (2) should receive one message
        clients[1].broadcast(&data).await.unwrap();

        let stream_0 = clients[0].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_0).await,
            Some(P2PMessage {
                sender_id: validator_ids[1].clone(),
                data: data.clone()
            })
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_2).await,
            Some(P2PMessage {
                sender_id: validator_ids[1].clone(),
                data: data.clone()
            })
        );
    }
}
