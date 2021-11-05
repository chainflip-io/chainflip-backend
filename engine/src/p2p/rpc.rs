use crate::common::into_anyhow_error;
use crate::p2p::{AccountId, P2PNetworkClient, StatusCode};
use anyhow::Result;
use async_trait::async_trait;
use cf_p2p::{AccountIdBs58, MessageBs58, P2PEvent, P2PRpcClient};
use futures::{stream::BoxStream, TryStreamExt};
use jsonrpc_core_client::RpcError;
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

pub async fn connect(url: &url::Url, validator_id: AccountId) -> Result<P2PRpcClient> {
    let client = jsonrpc_core_client::transports::ws::connect::<P2PRpcClient>(url)
        .await
        .map_err(|e| into_anyhow_error(RpcClientError::ConnectionError(url.clone(), e)))?;

    client
        .self_identify(AccountIdBs58(validator_id.0))
        .await
        .map_err(|e| {
            into_anyhow_error(RpcClientError::CallError(String::from("self_identify"), e))
        })?;

    Ok(client)
}

#[async_trait]
impl P2PNetworkClient for P2PRpcClient {
    type NetworkEvent = Result<P2PEvent>;

    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode> {
        P2PRpcClient::broadcast(self, MessageBs58(data.into()))
            .await
            .map_err(|e| into_anyhow_error(RpcClientError::CallError(String::from("broadcast"), e)))
    }

    async fn send(&self, to: &AccountId, data: &[u8]) -> Result<StatusCode> {
        P2PRpcClient::send(self, AccountIdBs58(to.0), MessageBs58(data.into()))
            .await
            .map_err(|e| into_anyhow_error(RpcClientError::CallError(String::from("send"), e)))
    }

    async fn take_stream(&self) -> Result<BoxStream<Self::NetworkEvent>> {
        let stream = self
            .subscribe_notifications()
            .map_err(|e| {
                into_anyhow_error(RpcClientError::CallError(
                    String::from("subscribe_notifications"),
                    e,
                ))
            })?
            .map_err(|e| into_anyhow_error(RpcClientError::SubscriptionError(e)));

        Ok(Box::pin(stream))
    }
}
