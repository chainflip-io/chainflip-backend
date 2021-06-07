use crate::{mq::IMQClient, settings};

use anyhow::Result;
use tokio_compat_02::FutureExt;

use super::Broadcast;

use async_trait::async_trait;

// Read events from the broadcast.eth queue, and then broadcast them

#[derive(Debug)]
pub struct EthBroadcaster<M: IMQClient + Send + Sync> {
    mq_client: M,
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
}

impl<M: IMQClient + Send + Sync> EthBroadcaster<M> {
    pub async fn new(settings: settings::Settings) -> Result<Self> {
        let mq_client = *M::connect(settings.message_queue).await?;

        let eth_node_ws_url = format!("ws://{}:{}", settings.eth.hostname, settings.eth.port);
        let transport = ::web3::transports::WebSocket::new(eth_node_ws_url.as_str())
            // TODO: Remove this compat once the websocket dep uses tokio1
            .compat()
            .await?;
        let web3_client = ::web3::Web3::new(transport);

        Ok(EthBroadcaster {
            mq_client,
            web3_client,
        })
    }
}

#[async_trait]
impl<M: IMQClient + Send + Sync> Broadcast for EthBroadcaster<M> {
    /// RLP encoded signed transaction
    async fn broadcast(msg: Vec<u8>) -> Result<String> {
        println!("Hello!");

        Ok("hello".to_string())
    }
}

#[cfg(test)]
mod tests {

    use crate::mq::nats_client::NatsMQClient;

    use super::*;

    pub async fn new_eth_broadcaster<M: IMQClient + Send + Sync>() -> Result<EthBroadcaster<M>> {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let eth_broadcaster = EthBroadcaster::<M>::new(settings).await;
        eth_broadcaster
    }

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let eth_broadcaster = new_eth_broadcaster::<NatsMQClient>().await;
        println!("{:#?}", eth_broadcaster);
        assert!(eth_broadcaster.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires eth node setup"]
    async fn test_eth_broadcast() {
        let eth_broadcaster = new_eth_broadcaster::<NatsMQClient>().await;
    }
}
