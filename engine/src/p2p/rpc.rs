use crate::p2p::{AccountId, P2PNetworkClient, StatusCode};
use anyhow::Result;
use async_trait::async_trait;
use cf_p2p_rpc::{AccountIdBs58, MessageBs58, P2PEvent, P2PRpcClient};
use futures::{stream::BoxStream, TryStreamExt};
use jsonrpc_core_client::{transports::ws, RpcError};
use thiserror::Error;

// This solves the silent failure where if an old style future tokio::spawns() it will cause our tokio runtime to shutdown
trait CompatFuture: futures::compat::Future01CompatExt + Sized {
    fn compat(self) -> tokio_compat_02::TokioContext<futures::compat::Compat01As03<Self>> {
        use futures::compat::Future01CompatExt; // This is a mostly 'simple' type type conversion
        use tokio_compat_02::FutureExt; // This will gives the future an old style tokio context, that internally uses the newer tokio runtime.
        FutureExt::compat(Future01CompatExt::compat(self))
    }
}
impl<T: futures::compat::Future01CompatExt> CompatFuture for T {}

/* If a stream internally tokio::spawns it will fail even using Stream01CompatExt::compat(), I don't think we
have this situation though. There doesn't seem to be an existing piece of code (tokio_compat_02::StreamExt::compat())
that wraps the stream and gives all its futures a TokioContext, alternatively we could use tokio_compat_02::FutureExt::compat
manualy on the next() future */
use futures::compat::Stream01CompatExt;

#[derive(Error, Debug)]
pub enum RpcClientError {
    #[error("Could not connect to {0:?}: {1:?}")]
    ConnectionError(url::Url, RpcError),
    #[error("Rpc error calling method {0:?}: {1:?}")]
    CallError(String, RpcError),
    #[error("Rpc subscription notified an error: {0:?}")]
    SubscriptionError(RpcError),
}

pub async fn connect(url: &url::Url, validator_id: AccountId) -> Result<P2PRpcClient> {
    let client = ws::connect::<P2PRpcClient>(url)
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
