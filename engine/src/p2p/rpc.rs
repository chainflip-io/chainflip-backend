use crate::p2p::{P2PNetworkClient, StatusCode, ValidatorId};
use anyhow::Result;
use async_trait::async_trait;
use cf_p2p_rpc::{MessageBs58, P2PEvent, P2pRpcClient, ValidatorIdBs58};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    stream::BoxStream,
    TryStreamExt,
};
use jsonrpc_core_client::{transports::ws, RpcError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RpcClientError {
    #[error("Could not connect to {0:?}: {1:?}")]
    ConnectionError(url::Url, RpcError),
    #[error("Rpc error calling method {0:?}: {1:?}")]
    CallError(String, RpcError),
    #[error("Rpc subscription notified an error: {0:?}")]
    SubscriptionError(RpcError),
}

pub async fn connect(url: &url::Url) -> Result<P2pRpcClient> {
    ws::connect::<P2pRpcClient>(url)
        .compat()
        .await
        .map_err(|e| RpcClientError::ConnectionError(url.clone(), e).into())
}

#[async_trait]
impl P2PNetworkClient for P2pRpcClient {
    type NetworkEvent = Result<P2PEvent>;

    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode> {
        P2pRpcClient::broadcast(self, MessageBs58(data.into()))
            .compat()
            .await
            .map_err(|e| RpcClientError::CallError(String::from("broadcast"), e).into())
    }

    async fn send(&self, to: &ValidatorId, data: &[u8]) -> Result<StatusCode> {
        P2pRpcClient::send(self, ValidatorIdBs58(to.0), MessageBs58(data.into()))
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
    use super::*;
    use cf_p2p_rpc::RpcApi;
    use jsonrpc_core::MetaIoHandler;
    use jsonrpc_core_client::transports::local;
    use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId};

    struct TestApi;

    impl RpcApi for TestApi {
        type Metadata = local::LocalMeta;

        fn identify(&self, validator_id: ValidatorIdBs58) -> jsonrpc_core::Result<u64> {
            Ok(200)
        }

        fn send(
            &self,
            validator_id: ValidatorIdBs58,
            message: MessageBs58,
        ) -> jsonrpc_core::Result<u64> {
            Ok(200)
        }

        fn broadcast(&self, message: MessageBs58) -> jsonrpc_core::Result<u64> {
            Ok(200)
        }

        fn subscribe_notifications(
            &self,
            metadata: Self::Metadata,
            subscriber: Subscriber<P2PEvent>,
        ) {
            todo!()
        }

        fn unsubscribe_notifications(
            &self,
            metadata: Option<Self::Metadata>,
            id: SubscriptionId,
        ) -> jsonrpc_core::Result<bool> {
            Ok(true)
        }
    }

    fn io() -> MetaIoHandler<local::LocalMeta> {
        let mut io = MetaIoHandler::default();
        io.extend_with(TestApi.to_delegate());
        io
    }

    #[test]
    fn client_api() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let io = io();
            let (client, s) = local::connect_with_pubsub::<P2pRpcClient, _>(&io);

            let result =
                P2PNetworkClient::send(&client, &ValidatorId([100; 32]), "disco".as_bytes()).await;

            assert!(
                result.is_ok(),
                "Should receive OK for sending message to peer"
            );

            let result = P2PNetworkClient::broadcast(&client, "disco".as_bytes()).await;

            assert!(result.is_ok(), "Should receive OK for broadcasting message");

            let result = P2PNetworkClient::take_stream(&client).await;
            assert!(result.is_ok(), "Should subscribe OK");
        });
    }
}
