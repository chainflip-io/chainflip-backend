use crate::mq::{pin_message_stream, IMQClient, Subject};
use crate::p2p::{P2PMessage, P2PNetworkClient, P2PNetworkClientError, StatusCode, ValidatorId};
use crate::state_chain::{auction::AuctionCompletedEvent, runtime::StateChainRuntime};
use async_trait::async_trait;
use cf_p2p_rpc::P2PEvent;
use futures::{Future, Stream, StreamExt};
use jsonrpc_core_client::transports::ws::connect;
use jsonrpc_core_client::{RpcChannel, RpcResult, TypedClient, TypedSubscriptionStream};
use libp2p_core::identity::ed25519;
use libp2p_core::{PeerId, PublicKey};
use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
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
                sender_id: self.inner.get(&peer_id).unwrap().clone(),
                data: msg,
            },
            P2PEvent::PeerConnected(peer_id) | P2PEvent::PeerDisconnected(peer_id) => P2PMessage {
                sender_id: self.inner.get(&peer_id).unwrap().clone(),
                data: vec![],
            },
        }
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

// TODO: this is duplicated in state-chain/client/cf-p2p/rpc/src/lib.rs
// TODO: Tests for this
fn peer_id_from_validator_id(validator_id: &String) -> std::result::Result<PeerId, &str> {
    sp_core::ed25519::Public::from_str(validator_id)
        .map_err(|_| "failed parsing")
        .and_then(|p| ed25519::PublicKey::decode(&p.0).map_err(|_| "failed decoding"))
        .and_then(|p| Ok(PeerId::from_public_key(PublicKey::Ed25519(p))))
}

#[derive(Clone)]
pub struct RpcP2PClient<IMQ>
where
    IMQ: IMQClient + Sync + Send + Clone,
{
    url: url::Url,
    peer_to_validator_mapper: RpcP2PClientMapper<IMQ>,
}

#[derive(Clone, Debug)]
pub struct RpcP2PClientMapper<IMQ>
where
    IMQ: IMQClient + Sync + Send + Clone,
{
    pub peer_to_validator_map: PeerIdValidatorMap,
    mq_client: IMQ,
}

impl<IMQ> RpcP2PClientMapper<IMQ>
where
    IMQ: IMQClient + Sync + Send + Clone,
{
    pub async fn init(mq_client: IMQ) -> Self {
        // HOW TO DO THIS??
        let auction_completed_event_stream = mq_client
            .subscribe::<AuctionCompletedEvent<StateChainRuntime>>(Subject::AuctionCompleted)
            .await
            .expect("Should be able to subscribe to Subject::AuctionCompleted");

        let mut auction_completed_event_stream = pin_message_stream(auction_completed_event_stream);

        let event = auction_completed_event_stream
            .next()
            .await
            .unwrap()
            .unwrap();

        let validator_ids: Vec<ValidatorId> =
            event.validators.iter().map(|a| a.clone().into()).collect();

        let mut peer_to_validator = HashMap::new();

        for id in validator_ids {
            println!("here's the id: {:?}", id);
            let peer_id =
                peer_id_from_validator_id(&id.to_ss58()).expect("Should be a valid validator id");
            // this is a different to_base58?
            println!("Peer id key: {}", peer_id.to_base58());
            peer_to_validator.insert(peer_id.to_base58(), id);
        }

        log::info!(
            "RpcP2PClientMapper received AuctionCompleted event: {:?}",
            event
        );

        let peer_to_validator_map = PeerIdValidatorMap {
            inner: peer_to_validator,
        };

        Self {
            peer_to_validator_map,
            // we still need this after initialisation to update after the next auction
            mq_client,
        }
    }

    // TODO: Should this be CFG test? don't think we'll need it in the end, but might
    // make testing a bit easier, without initialisation
    // pub fn new(mq_client: IMQ, validator_ids: Vec<ValidatorId>) -> Self {
    //     let mut peer_to_validator = HashMap::new();

    //     for id in validator_ids {
    //         println!("here's the id: {:?}", id);
    //         let peer_id =
    //             peer_id_from_validator_id(&id.to_ss58()).expect("Should be a valid validator id");
    //         // this is a different to_base58?
    //         println!("Peer id key: {}", peer_id.to_base58());
    //         peer_to_validator.insert(peer_id.to_base58(), id);
    //     }
    //     Self { peer_to_validator, mq_client: () }
    // }
}

impl<IMQ> RpcP2PClient<IMQ>
where
    IMQ: IMQClient + Sync + Send + Clone,
{
    pub fn new(url: url::Url, peer_to_validator_mapper: RpcP2PClientMapper<IMQ>) -> Self {
        RpcP2PClient {
            url,
            peer_to_validator_mapper,
        }
    }
}

pub struct RpcP2PClientStream {
    inner: Pin<Box<dyn Stream<Item = RpcResult<P2PEvent>> + Send>>,
    peer_to_validator_map: PeerIdValidatorMap,
}

impl RpcP2PClientStream {
    pub fn new(
        stream: TypedSubscriptionStream<P2PEvent>,
        peer_to_validator_map: PeerIdValidatorMap,
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
                        return Poll::Ready(Some(
                            self.peer_to_validator_map
                                .from_p2p_event_to_p2p_message(p2p_event),
                        ));
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
impl<NodeId, IMQ> P2PNetworkClient<NodeId, RpcP2PClientStream> for RpcP2PClient<IMQ>
where
    NodeId: Base58 + Send + Sync,
    IMQ: IMQClient + Send + Sync + Clone,
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
            self.peer_to_validator_mapper.peer_to_validator_map.clone(),
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

    #[test]
    fn client_api() {
        let server = TestServer::serve();
        let mut glue_client = RpcP2PClient::new(server.url, RpcP2PClientMapper::default());
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
        tokio::runtime::Runtime::new().unwrap().block_on(run);
    }

    #[test]
    fn validator_id_to_peer_id() {
        let peer_id = peer_id_from_validator_id(&ALICE_SS58.to_string()).unwrap();
        assert_eq!(peer_id.to_base58(), ALICE_PEER_ID);
    }

    fn create_new_mapping() -> Result<RpcP2PClientMapper> {
        let alice_validator = ValidatorId::from_ss58(ALICE_SS58)?;
        let validators = vec![alice_validator];
        let mapping = RpcP2PClientMapper::new(validators);
        Ok(mapping)
    }

    #[test]
    fn can_create_new_mapping() {
        assert!(create_new_mapping().is_ok());
    }

    #[test]
    fn p2p_event_is_mapped_to_p2p_message() {
        let mapping = create_new_mapping().unwrap();
        // we use Alice in the mapping constructor, so she'll be there
        let p2p_event_received = P2PEvent::PeerConnected(ALICE_PEER_ID.to_string());

        let expected_p2p_message = P2PMessage {
            sender_id: ValidatorId::from_ss58(ALICE_SS58).unwrap(),
            data: vec![],
        };

        let p2p_message = mapping.from_p2p_event_to_p2p_message(p2p_event_received);
        assert_eq!(p2p_message, expected_p2p_message);
    }
}
