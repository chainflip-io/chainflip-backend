use crate::mq::{pin_message_stream, IMQClient, Subject};
use crate::p2p::{P2PMessage, P2PNetworkClient, P2PNetworkClientError, StatusCode, ValidatorId};
use crate::settings;
use crate::state_chain::auction;
use crate::state_chain::{
    helpers::create_subxt_client, runtime::StateChainRuntime, session::ValidatorsStoreExt,
};
use async_trait::async_trait;
use cf_p2p_rpc::P2PEvent;
use futures::{Future, Stream, StreamExt};
use jsonrpc_core_client::transports::ws::connect;
use jsonrpc_core_client::{RpcChannel, RpcResult, TypedClient, TypedSubscriptionStream};
use libp2p_core::identity::ed25519;
use libp2p_core::{PeerId, PublicKey};
use sp_core::crypto::Ss58Codec;
use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use substrate_subxt::Client;
use tokio_compat_02::FutureExt;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct PeerIdValidatorMap {
    // base58 PeerId string to ValidatorId
    pub inner: HashMap<String, ValidatorId>,
}

impl PeerIdValidatorMap {
    pub fn from_p2p_event_to_p2p_message(&self, p2p_event: P2PEvent) -> P2PMessage {
        match p2p_event {
            P2PEvent::Received(peer_id, msg) => P2PMessage {
                sender_id: self
                    .inner
                    .get(&peer_id)
                    .expect(&format!("Peer id: {:?} should exist in mapping", peer_id))
                    .clone(),
                data: msg,
            },
            P2PEvent::PeerConnected(peer_id) | P2PEvent::PeerDisconnected(peer_id) => P2PMessage {
                sender_id: self
                    .inner
                    .get(&peer_id)
                    .expect(&format!("Peer id: {:?} should exist in mapping", peer_id))
                    .clone(),
                data: vec![],
            },
        }
    }

    /// Insert an item into the inner hashmap
    pub fn insert(&mut self, key: String, value: ValidatorId) -> Option<ValidatorId> {
        self.inner.insert(key, value)
    }
}

pub trait Base58 {
    fn to_base58(&self) -> String;
}

impl Base58 for () {
    fn to_base58(&self) -> String {
        "".to_string()
    }
}

pub trait SS58 {
    fn to_ss58(&self) -> String;
}

fn peer_id_from_validator_id(validator_id: &String) -> std::result::Result<PeerId, &str> {
    sp_core::ed25519::Public::from_str(validator_id)
        .map_err(|_| "failed parsing")
        .and_then(|p| ed25519::PublicKey::decode(&p.0).map_err(|_| "failed decoding"))
        .and_then(|p| Ok(PeerId::from_public_key(PublicKey::Ed25519(p))))
}

#[derive(Clone)]
pub struct RpcP2PClientMapper<IMQ>
where
    IMQ: IMQClient + Sync + Send + Clone,
{
    pub peer_to_validator_map: Arc<Mutex<PeerIdValidatorMap>>,
    mq_client: IMQ,
    subxt_client: Client<StateChainRuntime>,
}

impl<IMQ> RpcP2PClientMapper<IMQ>
where
    IMQ: IMQClient + Sync + Send + Clone,
{
    pub async fn init(state_chain_settings: &settings::StateChain, mq_client: IMQ) -> Result<Self> {
        log::info!("Initialising the RPCP2PClientMapper");
        let mut peer_to_validator_map: HashMap<String, ValidatorId> = HashMap::new();
        let subxt_client = create_subxt_client(&state_chain_settings).await?;
        let validators = subxt_client.validators(None).await?;

        for id in validators {
            let peer_id = peer_id_from_validator_id(&id.to_ss58check()).expect(&format!(
                "Should be a valid validator id: {:?}",
                id.to_ss58check()
            ));
            peer_to_validator_map.insert(peer_id.to_base58(), id.into());
        }

        let peer_to_validator_map = PeerIdValidatorMap {
            inner: peer_to_validator_map,
        };

        let peer_to_validator_map = Arc::new(Mutex::new(peer_to_validator_map));

        Ok(Self {
            peer_to_validator_map,
            // we still need this after initialisation to update after the next auction
            mq_client,
            subxt_client,
        })
    }

    pub fn clone_map(&self) -> Arc<Mutex<PeerIdValidatorMap>> {
        self.peer_to_validator_map.clone()
    }

    // We don't delete entries here. It's safer to just append the new ones and keep the old
    async fn update_map_with_sc(&self) -> Result<()> {
        let validators = self.subxt_client.validators(None).await?;

        for id in validators {
            let peer_id = peer_id_from_validator_id(&id.to_ss58check()).expect(&format!(
                "Should be a valid validator id: {:?}",
                id.to_ss58check()
            ));

            let mut map = self
                .peer_to_validator_map
                .lock()
                .expect("Should get lock to insert new peer mapping");
            map.insert(peer_id.to_base58(), id.into());
        }

        Ok(())
    }

    /// Keeps the mapping synced with the state chain
    pub async fn sync_validator_mapping(&self) {
        log::info!("Start synchronising the RpcP2PClientMapper");
        let auction_confirmed_stream = self
            .mq_client
            .subscribe::<auction::AuctionConfirmedEvent<StateChainRuntime>>(
                Subject::AuctionConfirmed,
            )
            .await
            .expect("Should subscribe to Subject::AuctionConfirmed");

        let auction_confirmed_stream = pin_message_stream(auction_confirmed_stream);
        auction_confirmed_stream
            .for_each_concurrent(None, |evt| async {
                evt.expect("Should be an event here");
                log::info!(
                    "Received AuctionConfirmed event, updating the Peer to Validator mapping"
                );
                // we received an event, so let's update the mapping
                self.update_map_with_sc()
                    .await
                    .expect("Should update successfully");
            })
            .await;
        log::error!("RpcP2PClientMapper synchronisation has stopped!")
    }

    #[cfg(test)]
    /// Convenience method for tests so we don't have to push an auction confirmed event to fill the validator map
    pub async fn new(
        state_chain_settings: &settings::StateChain,
        mq_client: IMQ,
        validator_ids: Vec<ValidatorId>,
    ) -> Self {
        let subxt_client = create_subxt_client(state_chain_settings).await.unwrap();

        let mut peer_to_validator_map = HashMap::new();

        for id in validator_ids {
            let peer_id = peer_id_from_validator_id(&id.to_ss58()).expect(&format!(
                "Should be a valid validator id: {:?}",
                id.to_ss58()
            ));
            peer_to_validator_map.insert(peer_id.to_base58(), id);
        }
        let peer_to_validator_map = PeerIdValidatorMap {
            inner: peer_to_validator_map,
        };
        let peer_to_validator_map = Arc::new(Mutex::new(peer_to_validator_map));
        Self {
            peer_to_validator_map,
            mq_client,
            subxt_client,
        }
    }
}

#[derive(Clone)]
pub struct RpcP2PClient {
    url: url::Url,
    peer_to_validator_map: Arc<Mutex<PeerIdValidatorMap>>,
}

impl RpcP2PClient {
    pub fn new(url: url::Url, peer_to_validator_map: Arc<Mutex<PeerIdValidatorMap>>) -> Self {
        RpcP2PClient {
            url,
            peer_to_validator_map,
        }
    }
}

pub struct RpcP2PClientStream {
    inner: Pin<Box<dyn Stream<Item = RpcResult<P2PEvent>> + Send>>,
    peer_to_validator_map: Arc<Mutex<PeerIdValidatorMap>>,
}

impl RpcP2PClientStream {
    pub fn new(
        stream: TypedSubscriptionStream<P2PEvent>,
        peer_to_validator_map: Arc<Mutex<PeerIdValidatorMap>>,
    ) -> Self {
        RpcP2PClientStream {
            inner: Box::pin(stream),
            peer_to_validator_map,
        }
    }
}

impl Stream for RpcP2PClientStream {
    type Item = P2PMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        loop {
            match this.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(result)) => {
                    if let Ok(p2p_event) = result {
                        let p2p_message = self
                            .peer_to_validator_map
                            .lock()
                            .expect("Should get lock on peer_to_validator_map")
                            .from_p2p_event_to_p2p_message(p2p_event);
                        return Poll::Ready(Some(p2p_message));
                    }
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => break,
            }
        }

        Poll::Pending
    }
}

#[async_trait]
impl<NodeId> P2PNetworkClient<NodeId, RpcP2PClientStream> for RpcP2PClient
where
    NodeId: Base58 + Send + Sync,
{
    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError> {
        let client: P2PClient = FutureExt::compat(connect(&self.url))
            .await
            .map_err(|_| P2PNetworkClientError::Rpc)?;

        client
            .broadcast(data.into())
            .await
            .map_err(|_| P2PNetworkClientError::Rpc)
    }

    async fn send(&self, to: &NodeId, data: &[u8]) -> Result<StatusCode, P2PNetworkClientError> {
        let client: P2PClient = FutureExt::compat(connect(&self.url))
            .await
            .map_err(|_| P2PNetworkClientError::Rpc)?;

        client
            .send(to.to_base58(), data.into())
            .await
            .map_err(|_| P2PNetworkClientError::Rpc)
    }

    async fn take_stream(&mut self) -> Result<RpcP2PClientStream, P2PNetworkClientError> {
        let client: P2PClient = FutureExt::compat(connect(&self.url))
            .await
            .map_err(|_| P2PNetworkClientError::Rpc)?;

        let sub = client
            .subscribe_notifications()
            .map_err(|_| P2PNetworkClientError::Rpc)?;

        Ok(RpcP2PClientStream::new(
            sub,
            self.peer_to_validator_map.clone(),
        ))
    }
}

#[derive(Clone)]
struct P2PClient {
    inner: TypedClient,
}

impl From<RpcChannel> for P2PClient {
    fn from(channel: RpcChannel) -> Self {
        P2PClient::new(channel.into())
    }
}

impl P2PClient {
    /// Creates a new `P2PClient`.
    pub fn new(sender: RpcChannel) -> Self {
        P2PClient {
            inner: sender.into(),
        }
    }
    /// Send a message to peer id returning a HTTP status code
    pub fn send(&self, peer_id: String, message: Vec<u8>) -> impl Future<Output = RpcResult<u64>> {
        let args = (peer_id, message);
        self.inner.call_method("p2p_send", "u64", args)
    }

    /// Broadcast a message to the p2p network returning a HTTP status code
    /// impl Future<Output = RpcResult<R>>
    pub fn broadcast(&self, message: Vec<u8>) -> impl Future<Output = RpcResult<u64>> {
        let args = (message,);
        self.inner.call_method("p2p_broadcast", "u64", args)
    }

    // Subscribe to receive notifications
    pub fn subscribe_notifications(&self) -> RpcResult<TypedSubscriptionStream<P2PEvent>> {
        let args_tuple = ();
        self.inner.subscribe(
            "cf_p2p_subscribeNotifications",
            args_tuple,
            "cf_p2p_notifications",
            "cf_p2p_unsubscribeNotifications",
            "RpcEvent",
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::mq::mq_mock::{MQMock, MQMockClient};

    use super::*;
    use jsonrpc_core::{IoHandler, Params};
    use jsonrpc_ws_server::{Server, ServerBuilder};
    use serde_json::json;

    struct TestServer {
        url: url::Url,
        server: Option<Server>,
    }

    const ALICE_SS58: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    const ALICE_PEER_ID: &str = "12D3KooWQ6jz4ttZfoBNKopouQWWkUVg93oWApg7ShLETzbnV3ec";

    impl TestServer {
        fn serve() -> Self {
            let server = ServerBuilder::new(io())
                .start(&"0.0.0.0:3030".parse().unwrap())
                .expect("This should start");

            TestServer {
                url: url::Url::parse("ws://127.0.0.1:3030").unwrap(),
                server: Some(server),
            }
        }
    }

    fn io() -> IoHandler {
        let mut io = IoHandler::default();
        io.add_sync_method("p2p_send", |params: Params| {
            match params.parse::<(String, Vec<u8>)>() {
                _ => Ok(json!(200)),
            }
        });
        io.add_sync_method("p2p_broadcast", |params: Params| {
            match params.parse::<(Vec<u8>,)>() {
                _ => Ok(json!(200)),
            }
        });

        io
    }

    // TODO: Work out a way we can mock the state chain so we don't have to ignore these tests
    #[tokio::test]
    async fn client_api() {
        let server = TestServer::serve();
        let mq_mock = MQMock::new();
        let mq_client = mq_mock.get_client();
        let mapper = create_new_mapper::<MQMockClient>(mq_client).await.unwrap();
        let map = mapper.clone_map();
        let mut glue_client = RpcP2PClient::new(server.url, map);
        let run = async {
            let result = glue_client
                .send(&ValidatorId::new("100"), "disco".as_bytes())
                .await;
            assert!(
                result.is_ok(),
                "Should receive OK for sending message to peer"
            );
            let result = P2PNetworkClient::<ValidatorId, RpcP2PClientStream>::broadcast(
                &glue_client,
                "disco".as_bytes(),
            )
            .await;
            assert!(result.is_ok(), "Should receive OK for broadcasting message");
            let result =
                P2PNetworkClient::<ValidatorId, RpcP2PClientStream>::take_stream(&mut glue_client)
                    .await;
            assert!(result.is_ok(), "Should subscribe OK");
        };
        run.await;
    }

    #[test]
    fn validator_id_to_peer_id() {
        let peer_id = peer_id_from_validator_id(&ALICE_SS58.to_string()).unwrap();
        assert_eq!(peer_id.to_base58(), ALICE_PEER_ID);

        let validator_id = "5G9NWJ5P9uk7am24yCKeLZJqXWW6hjuMyRJDmw4ofqxG8Js2";
        let expected_peer_id = "12D3KooWMxxmtYRoBr5yMGfXdunkZ3goE4fZsMuJJMRAm3UdySxg";

        let peer_id = peer_id_from_validator_id(&validator_id.to_string()).unwrap();
        assert_eq!(peer_id.to_base58(), expected_peer_id);
    }

    async fn create_new_mapper<IMQ: IMQClient + Send + Sync + Clone>(
        mq_mock_client: IMQ,
    ) -> Result<RpcP2PClientMapper<IMQ>> {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let alice_validator = ValidatorId::from_ss58(ALICE_SS58)?;
        let validators = vec![alice_validator];
        let mapper =
            RpcP2PClientMapper::new(&settings.state_chain, mq_mock_client, validators).await;
        Ok(mapper)
    }

    #[tokio::test]
    async fn can_create_new_mapping() {
        let mq_mock = MQMock::new();
        let mq_client = mq_mock.get_client();
        assert!(create_new_mapper::<MQMockClient>(mq_client).await.is_ok());
    }

    #[tokio::test]
    async fn p2p_event_is_mapped_to_p2p_message() {
        let mq_mock = MQMock::new();
        let mq_client = mq_mock.get_client();
        let mapper = create_new_mapper::<MQMockClient>(mq_client).await.unwrap();
        // we use Alice in the mapper constructor, so she'll be there
        let p2p_event_received = P2PEvent::PeerConnected(ALICE_PEER_ID.to_string());

        let expected_p2p_message = P2PMessage {
            sender_id: ValidatorId::from_ss58(ALICE_SS58).unwrap(),
            data: vec![],
        };

        let p2p_message = mapper
            .peer_to_validator_map
            .lock()
            .unwrap()
            .from_p2p_event_to_p2p_message(p2p_event_received);
        assert_eq!(p2p_message, expected_p2p_message);
    }
}
