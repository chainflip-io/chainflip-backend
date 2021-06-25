use crate::{
    eth::eth_tx_encoding::ContractCallDetails,
    mq::{pin_message_stream, IMQClient, Subject},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use web3::{Transport, Web3, ethabi::ethereum_types::H256, signing::SecretKeyRef, transports::WebSocket, types::TransactionParameters};

use futures::StreamExt;

/// Helper function, constructs and runs the [EthBroadcaster] asynchronously.
pub async fn start_eth_broadcaster<M: IMQClient + Send + Sync>(
    settings: &settings::Settings,
    mq_client: M,
) -> anyhow::Result<()> {
    let eth_broadcaster = EthBroadcaster::<M, _>::new(settings.into(), mq_client).await?;

    eth_broadcaster.run().await
}

/// Adapter struct to build the ethereum web3 client from settings.
struct EthClientBuilder {
    hostname: String,
    port: u16,
}

impl EthClientBuilder {
    pub fn new(hostname: String, port: u16) -> Self {
        Self { hostname, port }
    }

    /// Builds a web3 ethereum client with websocket transport.
    pub async fn ws_client(&self) -> Result<Web3<WebSocket>> {
        let url = format!("ws://{}:{}", self.hostname, self.port);
        let transport = web3::transports::WebSocket::new(url.as_str()).await?;
        Ok(Web3::new(transport))
    }
}

impl From<&settings::Settings> for EthClientBuilder {
    fn from(settings: &settings::Settings) -> Self {
        EthClientBuilder::new(settings.eth.hostname.clone(), settings.eth.port)
    }
}

/// Reads [ContractCallDetails] off the message queue and constructs, signs, and sends the tx to the ethereum network. 
#[derive(Debug)]
struct EthBroadcaster<M: IMQClient + Send + Sync, T: Transport> {
    mq_client: M,
    web3_client: Web3<T>,
}

impl<M: IMQClient + Send + Sync> EthBroadcaster<M, WebSocket> {
    async fn new(builder: EthClientBuilder, mq_client: M) -> Result<Self> {
        let web3_client = builder.ws_client().await?;

        Ok(EthBroadcaster {
            mq_client,
            web3_client,
        })
    }

    async fn run(&self) -> Result<()> {
        let subscription = self
            .mq_client
            .subscribe::<ContractCallDetails>(Subject::Broadcast(Chain::ETH))
            .await?;

        let subscription = pin_message_stream(subscription);

        subscription
            .for_each_concurrent(None, |msg| async {
                match msg {
                    Ok(ref tx_details) => {
                        match self.sign_and_broadcast(tx_details).await {
                            Ok(hash) => {
                                log::debug!("Transaction for {:?} broadcasted successfully: {:?}", tx_details, hash);
                            },
                            Err(err) => {
                                log::error!("Failed to broadcast transaction {:?}: {:?}", tx_details, err);
                            },
                        }
                    }
                    Err(e) => {
                        log::error!("Unable to broadcast message: {:?}.", e);
                    }
                }
        })
        .await;

        log::info!("{} has stopped.", stringify!(EthBroadcaster));
        Ok(())
    }

    /// Sign and broadcast a transaction
    async fn sign_and_broadcast(&self, tx_details: &ContractCallDetails) -> Result<H256> {
        let tx_params = TransactionParameters {
            to: Some(tx_details.contract_address),
            data: tx_details.data.clone().into(),
            .. Default::default()
        };

        let key = SecretKeyRef::new(todo!("Figure out how to get hold of this."));
        let signed = self.web3_client.accounts().sign_transaction(tx_params, key).await?;

        let tx_hash = self
            .web3_client
            .eth()
            .send_raw_transaction(signed.raw_transaction)
            .await?;
        
        // TODO: do we need something to tie the broadcasted item back to the original tx request?
        self.mq_client
            .publish(Subject::BroadcastSuccess(Chain::ETH), &tx_hash)
            .await?;

        Ok(tx_hash)
    }
}

#[cfg(test)]
mod tests {

    use crate::mq::{
        nats_client::{NatsMQClient, NatsMQClientFactory},
        IMQClientFactory,
    };

    use super::*;

    async fn new_eth_broadcaster() -> Result<EthBroadcaster<NatsMQClient, WebSocket>> {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let factory = NatsMQClientFactory::new(&settings.message_queue);
        let mq_client = *factory.create().await.unwrap();

        let eth_broadcaster = EthBroadcaster::<NatsMQClient, _>::new((&settings).into(), mq_client).await;
        eth_broadcaster
    }

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let eth_broadcaster = new_eth_broadcaster().await;
        assert!(eth_broadcaster.is_ok());
    }
}
