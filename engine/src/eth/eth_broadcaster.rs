use std::{path::Path, str::FromStr};

use crate::{
    eth::eth_tx_encoding::ContractCallDetails,
    mq::{pin_message_stream, IMQClient, Subject},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use secp256k1::SecretKey;
use web3::{
    ethabi::ethereum_types::H256, signing::SecretKeyRef, transports::WebSocket,
    types::TransactionParameters, Transport, Web3,
};

use futures::StreamExt;

/// Helper function, constructs and runs the [EthBroadcaster] asynchronously.
pub async fn start_eth_broadcaster<M: IMQClient + Send + Sync>(
    settings: &settings::Settings,
    mq_client: M,
) -> anyhow::Result<()> {
    log::info!("Starting ETH broadcaster");
    let secret_key = secret_key_from_file(Path::new(settings.eth.private_key_file.as_str()))?;
    let eth_broadcaster =
        EthBroadcaster::<M, _>::new(settings.into(), mq_client, secret_key).await?;

    eth_broadcaster.run().await
}

/// Retrieves a private key from a file. The file should contain just the hex-encoded key, nothing else.
fn secret_key_from_file(filename: &Path) -> Result<SecretKey> {
    let key = String::from_utf8(std::fs::read(filename)?)?;
    Ok(SecretKey::from_str(&key[..])?)
}

/// Adapter struct to build the ethereum web3 client from settings.
struct EthClientBuilder {
    node_endpoint: String,
}

impl EthClientBuilder {
    pub fn new(node_endpoint: String) -> Self {
        Self { node_endpoint }
    }

    /// Builds a web3 ethereum client with websocket transport.
    pub async fn ws_client(&self) -> Result<Web3<WebSocket>> {
        let transport = web3::transports::WebSocket::new(self.node_endpoint.as_str()).await?;
        Ok(Web3::new(transport))
    }
}

impl From<&settings::Settings> for EthClientBuilder {
    fn from(settings: &settings::Settings) -> Self {
        EthClientBuilder::new(settings.eth.node_endpoint.clone())
    }
}

/// Reads [ContractCallDetails] off the message queue and constructs, signs, and sends the tx to the ethereum network.
#[derive(Debug)]
struct EthBroadcaster<M: IMQClient + Send + Sync, T: Transport> {
    mq_client: M,
    web3_client: Web3<T>,
    secret_key: SecretKey,
}

impl<M: IMQClient + Send + Sync> EthBroadcaster<M, WebSocket> {
    async fn new(builder: EthClientBuilder, mq_client: M, secret_key: SecretKey) -> Result<Self> {
        let web3_client = builder.ws_client().await?;

        Ok(EthBroadcaster {
            mq_client,
            web3_client,
            secret_key,
        })
    }

    /// Consumes [TxDetails] messages from the `Broadcast` queue and signs and broadcasts the transaction to ethereum.
    async fn run(&self) -> Result<()> {
        let subscription = self
            .mq_client
            .subscribe::<ContractCallDetails>(Subject::Broadcast(Chain::ETH))
            .await?;

        let subscription = pin_message_stream(subscription);

        subscription
            .for_each_concurrent(None, |msg| async {
                match msg {
                    Ok(ref tx_details) => match self.sign_and_broadcast(tx_details).await {
                        Ok(hash) => {
                            log::debug!(
                                "Transaction for {:?} broadcasted successfully: {:?}",
                                tx_details,
                                hash
                            );
                        }
                        Err(err) => {
                            log::error!(
                                "Failed to broadcast transaction {:?}: {:?}",
                                tx_details,
                                err
                            );
                        }
                    },
                    Err(e) => {
                        log::error!("Unable to broadcast message: {:?}.", e);
                    }
                }
            })
            .await;

        log::error!("{} has stopped.", stringify!(EthBroadcaster));
        Ok(())
    }

    /// Sign and broadcast a transaction
    async fn sign_and_broadcast(&self, tx_details: &ContractCallDetails) -> Result<H256> {
        let tx_params = TransactionParameters {
            to: Some(tx_details.contract_address),
            data: tx_details.data.clone().into(),
            ..Default::default()
        };

        let key = SecretKeyRef::from(&self.secret_key);
        let signed = self
            .web3_client
            .accounts()
            .sign_transaction(tx_params, key)
            .await?;

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

    use crate::mq::nats_client::NatsMQClient;

    use super::*;

    async fn new_eth_broadcaster() -> Result<EthBroadcaster<NatsMQClient, WebSocket>> {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_client = NatsMQClient::new(&settings.message_queue).await.unwrap();
        let secret = SecretKey::from_slice(&[3u8; 32]).unwrap();

        let eth_broadcaster =
            EthBroadcaster::<NatsMQClient, _>::new((&settings).into(), mq_client, secret).await;
        eth_broadcaster
    }

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let eth_broadcaster = new_eth_broadcaster().await;
        assert!(eth_broadcaster.is_ok());
    }
}
