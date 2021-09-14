use std::collections::VecDeque;

use crate::p2p::{AccountId, P2PNetworkClient, StatusCode};
use anyhow::Result;
use async_trait::async_trait;
use cf_p2p_rpc::{AccountIdBs58, MessageBs58, P2PEvent, P2PRpcClient};
use failure::Error;
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    stream::BoxStream,
    TryStreamExt,
};
use jsonrpc_core::futures::{Async, AsyncSink, Future, Sink, Stream};
use jsonrpc_core_client::{
    transports::{duplex, ws},
    RpcChannel, RpcError,
};
use thiserror::Error;
use websocket::{ClientBuilder, OwnedMessage};

#[derive(Error, Debug)]
pub enum RpcClientError {
    #[error("Could not connect to {0:?}: {1:?}")]
    ConnectionError(url::Url, RpcError),
    #[error("Rpc error calling method {0:?}: {1:?}")]
    CallError(String, RpcError),
    #[error("Rpc subscription notified an error: {0:?}")]
    SubscriptionError(RpcError),
}

/// Connect to a JSON-RPC websocket server.
///
/// Uses an unbuffered channel to queue outgoing rpc messages.
pub fn inner_connect<T>(url: &url::Url) -> impl Future<Item = T, Error = RpcError>
where
    T: From<RpcChannel>,
{
    let client_builder = ClientBuilder::from_url(url);
    do_connect(client_builder)
}

fn do_connect<T>(client_builder: ClientBuilder) -> impl Future<Item = T, Error = RpcError>
where
    T: From<RpcChannel>,
{
    client_builder
        .async_connect(None)
        .map(|(client, _)| {
            let (sink, stream) = client.split();
            let (sink, stream) = WebsocketClient::new(sink, stream).split();
            let (rpc_client, sender) = duplex(sink, stream);
            let rpc_client = rpc_client.map_err(|error| eprintln!("{:?}", error));
            tokio::spawn(rpc_client.compat());
            sender.into()
        })
        .map_err(|error| RpcError::Other(error.into()))
}

struct WebsocketClient<TSink, TStream> {
    sink: TSink,
    stream: TStream,
    queue: VecDeque<OwnedMessage>,
}

impl<TSink, TStream, TError> WebsocketClient<TSink, TStream>
where
    TSink: Sink<SinkItem = OwnedMessage, SinkError = TError>,
    TStream: Stream<Item = OwnedMessage, Error = TError>,
    TError: Into<Error>,
{
    pub fn new(sink: TSink, stream: TStream) -> Self {
        Self {
            sink,
            stream,
            queue: VecDeque::new(),
        }
    }
}

impl<TSink, TStream, TError> Sink for WebsocketClient<TSink, TStream>
where
    TSink: Sink<SinkItem = OwnedMessage, SinkError = TError>,
    TStream: Stream<Item = OwnedMessage, Error = TError>,
    TError: Into<Error>,
{
    type SinkItem = String;
    type SinkError = RpcError;

    fn start_send(
        &mut self,
        request: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        self.queue.push_back(OwnedMessage::Text(request));
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            match self.queue.pop_front() {
                Some(request) => match self.sink.start_send(request) {
                    Ok(AsyncSink::Ready) => continue,
                    Ok(AsyncSink::NotReady(request)) => {
                        self.queue.push_front(request);
                        break;
                    }
                    Err(error) => return Err(RpcError::Other(error.into())),
                },
                None => break,
            }
        }
        self.sink
            .poll_complete()
            .map_err(|error| RpcError::Other(error.into()))
    }
}

impl<TSink, TStream, TError> Stream for WebsocketClient<TSink, TStream>
where
    TSink: Sink<SinkItem = OwnedMessage, SinkError = TError>,
    TStream: Stream<Item = OwnedMessage, Error = TError>,
    TError: Into<Error>,
{
    type Item = String;
    type Error = RpcError;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        loop {
            match self.stream.poll() {
                Ok(Async::Ready(Some(message))) => match message {
                    OwnedMessage::Text(data) => return Ok(Async::Ready(Some(data))),
                    OwnedMessage::Binary(data) => (),
                    OwnedMessage::Ping(p) => self.queue.push_front(OwnedMessage::Pong(p)),
                    OwnedMessage::Pong(_) => {}
                    OwnedMessage::Close(c) => self.queue.push_front(OwnedMessage::Close(c)),
                },
                Ok(Async::Ready(None)) => {
                    // TODO try to reconnect (#411).
                    return Ok(Async::Ready(None));
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(error) => return Err(RpcError::Other(error.into())),
            }
        }
    }
}

pub async fn connect(url: &url::Url, validator_id: AccountId) -> Result<P2PRpcClient> {
    let client = inner_connect::<P2PRpcClient>(url)
        .compat()
        .await
        .map_err(|e| RpcClientError::ConnectionError(url.clone(), e))?;

    client
        .self_identify(AccountIdBs58(validator_id.0))
        .compat()
        .await
        .map_err(|e| RpcClientError::CallError(String::from("identify"), e))?;

    Ok(client)
}

#[async_trait]
impl P2PNetworkClient for P2PRpcClient {
    type NetworkEvent = Result<P2PEvent>;

    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode> {
        P2PRpcClient::broadcast(self, MessageBs58(data.into()))
            .compat()
            .await
            .map_err(|e| RpcClientError::CallError(String::from("broadcast"), e).into())
    }

    async fn send(&self, to: &AccountId, data: &[u8]) -> Result<StatusCode> {
        P2PRpcClient::send(self, AccountIdBs58(to.0), MessageBs58(data.into()))
            .compat()
            .await
            .map_err(|e| RpcClientError::CallError(String::from("send"), e).into())
    }

    async fn take_stream(&self) -> Result<BoxStream<Self::NetworkEvent>> {
        let stream = self
            .subscribe_notifications()
            .compat()
            .await
            .map_err(|e| RpcClientError::CallError(String::from("subscribe_notifications"), e))?
            .compat()
            .map_err(|e| RpcClientError::SubscriptionError(e).into());

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use super::*;
    use cf_p2p_rpc::RpcApi;
    use jsonrpc_core::MetaIoHandler;
    use jsonrpc_core_client::transports::local;
    use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId};

    #[derive(Default)]
    struct TestApi {
        subs: Arc<Mutex<HashMap<SubscriptionId, jsonrpc_pubsub::typed::Sink<P2PEvent>>>>,
    }

    impl RpcApi for TestApi {
        type Metadata = local::LocalMeta;

        fn self_identify(&self, _validator_id: AccountIdBs58) -> jsonrpc_core::Result<u64> {
            Ok(200)
        }

        fn send(
            &self,
            _validator_id: AccountIdBs58,
            _message: MessageBs58,
        ) -> jsonrpc_core::Result<u64> {
            Ok(200)
        }

        fn broadcast(&self, _message: MessageBs58) -> jsonrpc_core::Result<u64> {
            Ok(200)
        }

        fn subscribe_notifications(
            &self,
            _metadata: Self::Metadata,
            subscriber: Subscriber<P2PEvent>,
        ) {
            let mut subs = self.subs.lock().unwrap();
            let next = SubscriptionId::Number(subs.len() as u64 + 1);
            let sink = subscriber.assign_id(next.clone()).unwrap();
            subs.insert(next, sink);
        }

        fn unsubscribe_notifications(
            &self,
            _metadata: Option<Self::Metadata>,
            id: SubscriptionId,
        ) -> jsonrpc_core::Result<bool> {
            self.subs.lock().unwrap().remove(&id).unwrap();
            Ok(true)
        }
    }

    fn io() -> MetaIoHandler<local::LocalMeta> {
        let mut io = MetaIoHandler::default();
        io.extend_with(TestApi::default().to_delegate());
        io
    }

    #[test]
    fn client_api() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let io = io();
            let (client, server) = local::connect_with_pubsub::<P2PRpcClient, _>(&io);

            tokio::select! {
                _ = async move {
                    let result =
                        P2PNetworkClient::send(&client, &AccountId([100; 32]), "disco".as_bytes()).await;
                    assert!(
                        result.is_ok(),
                        "Should receive OK for sending message to peer"
                    );
                    let result = P2PNetworkClient::broadcast(&client, "disco".as_bytes()).await;
                    assert!(result.is_ok(), "Should receive OK for broadcasting message");
                    let result = P2PNetworkClient::take_stream(&client).await;
                    assert!(result.is_ok(), "Should subscribe OK");
                } => {}
                _ = server.compat() => {}
            };
        });
    }
}
