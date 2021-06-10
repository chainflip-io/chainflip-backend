use crate::{
    mq::{pin_message_stream, IMQClient, Subject},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use tokio_compat_02::FutureExt;

use super::Broadcast;

use async_trait::async_trait;

use futures::StreamExt;

pub async fn start_eth_broadcaster<M: IMQClient + Send + Sync>(
    settings: settings::Settings,
) -> anyhow::Result<()> {
    let eth_broadcaster = EthBroadcaster::<M>::new(settings).await?;

    eth_broadcaster.run().await?;

    Ok(())
}

#[derive(Debug)]
pub struct EthBroadcaster<M: IMQClient + Send + Sync> {
    mq_client: M,
    web3_client: ::web3::Web3<::web3::transports::WebSocket>,
}

impl<M: IMQClient + Send + Sync> EthBroadcaster<M> {
    async fn new(settings: settings::Settings) -> Result<Self> {
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

    async fn run(&self) -> Result<()> {
        let stream = self
            .mq_client
            .subscribe::<Vec<u8>>(Subject::Broadcast(Chain::ETH))
            .await?;

        let mut stream = pin_message_stream(stream);

        while let Some(msg) = stream.next().await {
            match msg {
                Ok(msg) => {
                    let _tx_hash = self.broadcast(msg).await?;
                }
                Err(err) => {
                    log::error!("Error reading next item from broadcast queue: {:?}", err);
                }
            }
        }

        log::info!("ETH broadcaster has stopped");
        Ok(())
    }
}

#[async_trait]
impl<M: IMQClient + Send + Sync> Broadcast for EthBroadcaster<M> {
    /// Broadcast an RLP encoded signed transaction
    async fn broadcast(&self, tx: Vec<u8>) -> Result<String> {
        // sends raw transaction and waits for transaction to be confirmed - in this case we just
        // return the hash immediately - the state chain handles stalling
        let tx_hash = self
            .web3_client
            .eth()
            .send_raw_transaction(tx.into())
            .await?;

        self.mq_client
            .publish(Subject::BroadcastSuccess(Chain::ETH), &tx_hash)
            .await?;

        Ok(tx_hash.to_string())
    }
}

#[cfg(test)]
mod tests {

    use crate::mq::nats_client::NatsMQClient;

    use super::*;

    // NB: These two transactions depend on having a ganache network using the mnemonic `chainflip`
    // These also have to be run on a fresh instance of ganache, and executed in order
    // 1. test_eth_broadcast_success() and 2. test_eth_broadcast_revert()
    // , since the nonces for these txs must

    // A successful tx that will send 1.2345e-14 ETH from 0x9dbE382B57bCdc2aAbC874130E120a3E7dE09bDa to 0x4726b1555bF7AB73553Be4eb3cfE15376D0dB188:
    static SUCCESS_TX: &str = "f866808504a817c800825208944726b1555bf7ab73553be4eb3cfe15376d0db188823039801ca0b68916d2dc645ee4555c83bcdf40ac259b3e6f2ca78bcf56bff0c04466655b27a061ecc588392aa2ea755a5e05dc4ad3ea3a67d569d920de16c9b408255153301f";

    // A reverting tx that will fail trying to send 1000.0 ETH from 0x9dbE382B57bCdc2aAbC874130E120a3E7dE09bDa to 0x55024FA7C8217B88d16B240d09F76C6581245a94:
    static REVERTING_TX: &str = "f86d018504a817c8008252089455024fa7c8217b88d16b240d09f76c6581245a94893635c9adc5dea00000801ca049c86a1429efcd6de51c7a27d65d58690ec77c133b60cb492cb3c693f097fb23a0790061e0df154f4e82984d641afbb66d127a90fab66c555b69cb718f81538367";

    pub async fn new_eth_broadcaster<M: IMQClient + Send + Sync>() -> Result<EthBroadcaster<M>> {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let eth_broadcaster = EthBroadcaster::<M>::new(settings).await;
        eth_broadcaster
    }

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let eth_broadcaster = new_eth_broadcaster::<NatsMQClient>().await;
        assert!(eth_broadcaster.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires fresh eth node setup with `chainflip` mnemonic"]
    async fn test_eth_broadcast_success() {
        let eth_broadcaster = new_eth_broadcaster::<NatsMQClient>().await.unwrap();

        let bytes = hex::decode(SUCCESS_TX).unwrap();

        let result = eth_broadcaster.broadcast(bytes).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires fresh eth node setup with `chainflip` mnemonic"]
    async fn test_eth_broadcast_revert() {
        let eth_broadcaster = new_eth_broadcaster::<NatsMQClient>().await.unwrap();

        let bytes = hex::decode(REVERTING_TX).unwrap();

        let result = eth_broadcaster.broadcast(bytes).await;

        // Should fail as we are trying to send more funds than we have.
        assert!(result.is_err());
    }
}
