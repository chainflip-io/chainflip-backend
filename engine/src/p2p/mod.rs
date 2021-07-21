// Note: we temporary allow mock in non-test code
#[cfg(test)]
pub mod mock;

mod conductor;
mod rpc;

use std::{convert::TryInto, str::FromStr};

pub use conductor::P2PConductor;
pub use rpc::{RpcP2PClient, RpcP2PClientMapping};

use serde::{Deserialize, Serialize};
use sp_core::ed25519::Public;

use async_trait::async_trait;
use futures::Stream;
use rpc::{Base58, SS58};

use sp_core::crypto::Ss58Codec;

use anyhow::Result;

use sp_runtime::AccountId32;
use state_chain_runtime::SS58Prefix;

use crate::state_chain::validator::Validator;

#[derive(Debug)]
pub enum P2PNetworkClientError {
    Format,
    Rpc,
}

type StatusCode = u64;

#[async_trait]
pub trait P2PNetworkClient<B: Base58, S: Stream<Item = P2PMessage>> {
    /// Broadcast to all validators on the network
    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError>;

    /// Send to a specific `validator` only
    async fn send(&self, to: &B, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError>;

    async fn take_stream(&mut self) -> Result<S, P2PNetworkClientError>;
}

/// What is this???? We need a comment here
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Eq, PartialOrd, Ord, Hash)]
pub struct ValidatorId(pub [u8; 32]);

impl ValidatorId {
    // A convenience method to quickly generate different validator ids
    // from a string of any size that is no larger that 32 bytes
    #[cfg(test)]
    pub fn new<T: ToString>(id: T) -> Self {
        let id_str = id.to_string();
        let id_bytes = id_str.as_bytes();

        let mut id: [u8; 32] = [0; 32];

        for (idx, byte) in id_bytes.iter().enumerate() {
            id[idx] = *byte;
        }

        ValidatorId(id)
    }

    pub fn from_base58(id: &str) -> Result<Self> {
        todo!("Remove this?");
        let id = bs58::decode(&id)
            .into_vec()
            .map_err(|_| anyhow::format_err!("Id is not valid base58: {}", id))?;

        println!("The id len is: {}", id.len());
        println!("The id is: {:?}", id);
        let id = id
            .try_into()
            .map_err(|_| anyhow::format_err!("Id is not 32 bytes"))?;

        Ok(ValidatorId(id))
    }

    pub fn from_ss58(id: &str) -> Result<Self> {
        let public =
            Public::from_str(id).map_err(|_| anyhow::format_err!("Not valid ss58 id: {}", id))?;
        Ok(ValidatorId(public.0))
    }
}

impl std::fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ValidatorId({})", self.to_base58())
    }
}

// TODO: Can this be removed?
impl Base58 for ValidatorId {
    fn to_base58(&self) -> String {
        bs58::encode(&self.0).into_string()
    }
}

impl SS58 for ValidatorId {
    fn to_ss58(&self) -> String {
        let account_id = AccountId32::new(self.0);
        let ss58 = account_id.to_ss58check();
        return ss58;
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

    use itertools::Itertools;
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::mock::*;
    use super::*;

    const ALICE_SS58: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    const ALICE_PUBKEY: &str = "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";

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
        let validator_ids = (0..3).map(|i| ValidatorId::new(i)).collect_vec();

        let mut clients = validator_ids
            .iter()
            .map(|id| network.new_client(id.clone()))
            .collect_vec();

        // (0) sends to (1); (1) should receive one, (2) receives none
        clients[0].send(&validator_ids[1], &data).await.unwrap();

        drop(network);

        let stream_1 = clients[1].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_1.into_inner()).await,
            Some(P2PMessage {
                sender_id: validator_ids[0].clone(),
                data: data.clone()
            })
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(receive_with_timeout(stream_2.into_inner()).await, None);
    }

    #[tokio::test]
    async fn test_p2p_mock_broadcast() {
        let network = NetworkMock::new();

        let data = vec![3, 2, 1];
        let validator_ids = (0..3).map(|i| ValidatorId::new(i)).collect_vec();
        let mut clients = validator_ids
            .iter()
            .map(|id| network.new_client(id.clone()))
            .collect_vec();

        // (1) broadcasts; (0) and (2) should receive one message
        clients[1].broadcast(&data).await.unwrap();

        let stream_0 = clients[0].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_0.into_inner()).await,
            Some(P2PMessage {
                sender_id: validator_ids[1].clone(),
                data: data.clone()
            })
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_2.into_inner()).await,
            Some(P2PMessage {
                sender_id: validator_ids[1].clone(),
                data: data.clone()
            })
        );
    }

    #[test]
    fn validator_id_to_ss58() {
        let pubkey_bytes: [u8; 32] = hex::decode(ALICE_PUBKEY).unwrap().try_into().unwrap();
        let validator_id = ValidatorId(pubkey_bytes);
        assert_eq!(validator_id.to_ss58(), ALICE_SS58);
    }

    #[test]
    fn validator_id_from_ss58() {
        let pubkey_bytes: [u8; 32] = hex::decode(ALICE_PUBKEY).unwrap().try_into().unwrap();

        let validator_id = ValidatorId::from_ss58(ALICE_SS58).unwrap();

        assert_eq!(validator_id.0, pubkey_bytes);
    }
}
