use std::{path::Path, str::FromStr};

use crate::{
    eth::eth_tx_encoding::ContractCallDetails,
    logging::COMPONENT_KEY,
    mq::{IMQClient, Subject},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use secp256k1::SecretKey;
use slog::o;
use std::time::Duration;
use web3::{
    ethabi::ethereum_types::H256, signing::SecretKeyRef, transports::WebSocket,
    types::TransactionParameters, Transport, Web3,
};

use futures::StreamExt;

/// Helper function, constructs and runs the [EthBroadcaster] asynchronously.
pub async fn start_eth_broadcaster<M: IMQClient + Send + Sync>(
    settings: &settings::Settings,
    mq_client: M,
    logger: &slog::Logger,
) {
    EthBroadcaster::<M, _>::new(
        &settings,
        mq_client,
        secret_key_from_file(Path::new(settings.eth.private_key_file.as_str())).expect(&format!(
            "Should read in secret key from: {}",
            settings.eth.private_key_file,
        )),
        logger,
    )
    .await
    .expect("Should create eth broadcaster")
    .run()
    .await
    .expect("Should run eth broadcaster");
}

/// Retrieves a private key from a file. The file should contain just the hex-encoded key, nothing else.
fn secret_key_from_file(filename: &Path) -> Result<SecretKey> {
    let key = String::from_utf8(std::fs::read(filename)?)?;
    Ok(SecretKey::from_str(&key[..])?)
}

/// Reads [ContractCallDetails] off the message queue and constructs, signs, and sends the tx to the ethereum network.
#[derive(Debug)]
struct EthBroadcaster<M: IMQClient + Send + Sync, T: Transport> {
    mq_client: M,
    web3_client: Web3<T>,
    secret_key: SecretKey,
    logger: slog::Logger,
}

impl<M: IMQClient + Send + Sync> EthBroadcaster<M, WebSocket> {
    async fn new(
        settings: &settings::Settings,
        mq_client: M,
        secret_key: SecretKey,
        logger: &slog::Logger,
    ) -> Result<Self> {
        slog::debug!(
            logger,
            "Connecting new Eth Broadcaster to {}",
            settings.eth.node_endpoint.as_str()
        );
        match tokio::time::timeout(
            Duration::from_secs(5),
            web3::transports::WebSocket::new(settings.eth.node_endpoint.as_str()),
        )
        .await
        {
            Ok(Ok(socket)) => {
                // Successful connection
                Ok(Self {
                    mq_client,
                    web3_client: Web3::new(socket),
                    secret_key,
                    logger: logger.new(o!(COMPONENT_KEY => "ETHBroadcaster")),
                })
            }
            Ok(Err(e)) => {
                // Connection error
                Err(e.into())
            }
            Err(_) => {
                // Connection timeout
                Err(anyhow::Error::msg(format!(
                    "Timeout connecting to {:?}",
                    settings.eth.node_endpoint.as_str()
                )))
            }
        }
    }

    /// Consumes [TxDetails] messages from the `Broadcast` queue and signs and broadcasts the transaction to ethereum.
    async fn run(&self) -> Result<()> {
        slog::info!(self.logger, "Starting");
        let subscription = self
            .mq_client
            .subscribe::<ContractCallDetails>(Subject::Broadcast(Chain::ETH))
            .await?;

        subscription
            .for_each_concurrent(None, |msg| async {
                match msg {
                    Ok(ref tx_details) => match self.sign_and_broadcast(tx_details).await {
                        Ok(hash) => {
                            slog::debug!(
                                self.logger,
                                "Transaction for {:?} broadcasted successfully: {:?}",
                                tx_details,
                                hash
                            );
                        }
                        Err(err) => {
                            slog::error!(
                                self.logger,
                                "Failed to broadcast transaction {:?}: {:?}",
                                tx_details,
                                err
                            );
                        }
                    },
                    Err(e) => {
                        slog::error!(self.logger, "Unable to broadcast message: {:?}.", e);
                    }
                }
            })
            .await;

        slog::error!(self.logger, "{} has stopped.", stringify!(EthBroadcaster));
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

    use crate::{logging, mq::nats_client::NatsMQClient};

    use super::*;

    async fn new_eth_broadcaster() -> Result<EthBroadcaster<NatsMQClient, WebSocket>> {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_client = NatsMQClient::new(&settings.message_queue).await.unwrap();
        let secret = SecretKey::from_slice(&[3u8; 32]).unwrap();
        let logger = logging::test_utils::create_test_logger();

        let eth_broadcaster =
            EthBroadcaster::<NatsMQClient, _>::new(&settings, mq_client, secret, &logger).await;
        eth_broadcaster
    }

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let eth_broadcaster = new_eth_broadcaster().await;
        assert_ok!(eth_broadcaster);
    }
}
