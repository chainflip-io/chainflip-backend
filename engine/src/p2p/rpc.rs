use crate::common::rpc_error_into_anyhow_error;
use crate::p2p::{AccountId, P2PNetworkClient, StatusCode};
use anyhow::{Context, Result};
use async_trait::async_trait;
use cf_p2p::{AccountIdBs58, MessageBs58, P2PEvent, P2PRpcClient};
use futures::{stream::BoxStream, TryStreamExt};

pub async fn connect(url: &url::Url, validator_id: AccountId) -> Result<P2PRpcClient> {
    let client = jsonrpc_core_client::transports::ws::connect::<P2PRpcClient>(url)
        .await
        .map_err(rpc_error_into_anyhow_error)
        .context("connect")?;

    client
        .self_identify(AccountIdBs58(validator_id.0))
        .await
        .map_err(rpc_error_into_anyhow_error)
        .context("self_identify")?;

    Ok(client)
}

#[async_trait]
impl P2PNetworkClient for P2PRpcClient {
    type NetworkEvent = Result<P2PEvent>;

    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode> {
        P2PRpcClient::broadcast(self, MessageBs58(data.into()))
            .await
            .map_err(rpc_error_into_anyhow_error)
            .context("broadcast")
    }

    async fn send(&self, to: &AccountId, data: &[u8]) -> Result<StatusCode> {
        P2PRpcClient::send(self, AccountIdBs58(to.0), MessageBs58(data.into()))
            .await
            .map_err(rpc_error_into_anyhow_error)
            .context("send")
    }

    async fn take_stream(&self) -> Result<BoxStream<Self::NetworkEvent>> {
        let stream = self
            .subscribe_notifications()
            .map_err(rpc_error_into_anyhow_error)
            .context("subscribe_notifications")?
            .map_err(rpc_error_into_anyhow_error);

        Ok(Box::pin(stream))
    }
}
