use crate::p2p::{P2PMessage, P2PNetworkClient, P2PNetworkClientError, StatusCode, ValidatorId};
use async_trait::async_trait;
use cf_p2p_rpc::P2PEvent;
use futures::{Future, Stream, StreamExt};
use jsonrpc_core_client::transports::ws::connect;
use jsonrpc_core_client::{RpcChannel, RpcResult, TypedClient, TypedSubscriptionStream};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_compat_02::FutureExt;

pub trait Base58 {
    fn to_base58(&self) -> String;
}

impl Base58 for () {
    fn to_base58(&self) -> String {
        "".to_string()
    }
}

pub struct RpcP2PClient {
    url: url::Url,
}

impl RpcP2PClient {
    pub fn new(url: url::Url) -> Self {
        RpcP2PClient { url }
    }
}

impl From<P2PEvent> for P2PMessage {
    fn from(p2p_event: P2PEvent) -> Self {
        match p2p_event {
            P2PEvent::Received(peer_id, msg) => P2PMessage {
                sender_id: ValidatorId::from_base58(&peer_id)
                    .expect("valid 58 encoding of peer id"),
                data: msg,
            },
            P2PEvent::PeerConnected(peer_id) | P2PEvent::PeerDisconnected(peer_id) => P2PMessage {
                sender_id: ValidatorId::from_base58(&peer_id)
                    .expect("valid 58 encoding of peer id"),
                data: vec![],
            },
        }
    }
}

pub struct RpcP2PClientStream {
    inner: Pin<Box<dyn Stream<Item = RpcResult<P2PEvent>> + Send>>,
}

impl RpcP2PClientStream {
    pub fn new(stream: TypedSubscriptionStream<P2PEvent>) -> Self {
        RpcP2PClientStream {
            inner: Box::pin(stream),
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
                    if let Ok(result) = result {
                        return Poll::Ready(Some(result.into()));
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

        Ok(RpcP2PClientStream::new(sub))
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

const U64_RPC_TYPE: &str = "u64";

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
        self.inner.call_method("p2p_send", U64_RPC_TYPE, args)
    }

    /// Broadcast a message to the p2p network returning a HTTP status code
    /// impl Future<Output = RpcResult<R>>
    pub fn broadcast(&self, message: Vec<u8>) -> impl Future<Output = RpcResult<u64>> {
        let args = (message,);
        self.inner.call_method("p2p_broadcast", U64_RPC_TYPE, args)
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
        let mut glue_client = RpcP2PClient::new(server.url);
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
}
