use std::{path::Path, str::FromStr};

use crate::{
    logging::COMPONENT_KEY,
    mq::{IMQClient, Subject},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use secp256k1::SecretKey;
use slog::o;
use web3::{
    ethabi::ethereum_types::H256, signing::SecretKeyRef, types::TransactionParameters, Transport,
    Web3,
};

use serde::{Deserialize, Serialize};
use web3::types::Address;

use futures::StreamExt;

/// Details of a contract call to be broadcast to ethereum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ContractCallDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}

/// Helper function, constructs and runs the [EthBroadcaster] asynchronously.
pub async fn start_eth_broadcaster<T: Transport, M: IMQClient + Send + Sync>(
    web3: &Web3<T>,
    settings: &settings::Settings,
    mq_client: M,
    logger: &slog::Logger,
) {
    EthBroadcaster::<M, _>::new(
        web3,
        mq_client,
        secret_key_from_file(settings.eth.private_key_file.as_path()).expect(&format!(
            "Should read in secret key from: {}",
            settings.eth.private_key_file.display(),
        )),
        logger,
    )
    .await
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
    web3: Web3<T>,
    secret_key: SecretKey,
    logger: slog::Logger,
}

impl<T: Transport, M: IMQClient + Send + Sync> EthBroadcaster<M, T> {
    async fn new(
        web3: &Web3<T>,
        mq_client: M,
        secret_key: SecretKey,
        logger: &slog::Logger,
    ) -> Self {
        Self {
            mq_client,
            web3: web3.clone(),
            secret_key,
            logger: logger.new(o!(COMPONENT_KEY => "ETHBroadcaster")),
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
            .web3
            .accounts()
            .sign_transaction(tx_params, key)
            .await?;

        let tx_hash = self
            .web3
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

    use crate::eth;
    use crate::{logging, mq::nats_client::NatsMQClient};
    use web3::transports::WebSocket;

    use super::*;

    async fn new_eth_broadcaster() -> Result<EthBroadcaster<NatsMQClient, WebSocket>> {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_client = NatsMQClient::new(&settings.message_queue).await.unwrap();
        let secret = SecretKey::from_slice(&[3u8; 32]).unwrap();
        let logger = logging::test_utils::create_test_logger();

        Ok(EthBroadcaster::<NatsMQClient, _>::new(
            &eth::new_synced_web3_client(&settings, &logger).await?,
            mq_client,
            secret,
            &logger,
        )
        .await)
    }

    #[tokio::test]
    #[ignore = "requires mq and eth node setup"]
    async fn test_eth_broadcaster_new() {
        let eth_broadcaster = new_eth_broadcaster().await;
        assert_ok!(eth_broadcaster);
    }
}
