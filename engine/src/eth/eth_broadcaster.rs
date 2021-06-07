use crate::{mq::IMQClient, settings};

use anyhow::Result;
use tokio_compat_02::FutureExt;

// Read events from the broadcast.eth queue, and then broadcast them

pub struct EthBroadcaster<M: IMQClient + Send + Sync> {
    mq_client: M,
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
}

impl<M: IMQClient + Send + Sync> EthBroadcaster<M> {
    pub async fn new(settings: settings::Settings) -> Result<Self> {
        let mq_client = *M::connect(settings.message_queue).await?;

        let eth_node_ws_url = format!("{}:{}", settings.eth.hostname, settings.eth.port);
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

#[cfg(test)]
mod tests {

    use crate::mq::nats_client::NatsMQClient;

    use super::*;

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let eth_broadcaster = EthBroadcaster::<NatsMQClient>::new(settings).await;
        assert!(eth_broadcaster.is_ok());
    }
}
