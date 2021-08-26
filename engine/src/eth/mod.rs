pub mod key_manager;
pub mod stake_manager;

pub mod eth_event_streamer;

pub mod eth_broadcaster;
pub mod utils;

use anyhow::Result;

use secp256k1::SecretKey;
use thiserror::Error;
use web3::ethabi::{Address, Contract, Event};
use web3::signing::SecretKeyRef;
use web3::types::{SyncState, TransactionParameters};

use crate::settings;
use futures::TryFutureExt;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use web3::types::H256;
use web3::{Transport, Web3};

use serde::{Deserialize, Serialize};

#[derive(Error, Debug)]
pub enum EventParseError {
    #[error("Unexpected event signature in log subscription: {0:#}")]
    UnexpectedEvent(H256),
    #[error("Cannot decode missing parameter: '{0}'.")]
    MissingParam(String),
}

// The signature is recalculated on each Event::signature() call, so we use this structure to cache the signture-
pub struct SignatureAndEvent {
    pub signature: H256,
    pub event: Event,
}
impl SignatureAndEvent {
    pub fn new(contract: &Contract, name: &str) -> Result<Self> {
        let event = contract.event(name)?;
        Ok(Self {
            signature: event.signature(),
            event: event.clone(),
        })
    }
}

/// Details of a contract call to be broadcast to ethereum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ContractCallDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}

/// Enables ETH event streaming via the `Web3` client and signing & broadcasting of txs
#[derive(Clone, Debug)]
pub struct Web3Signer {
    web3: Web3<web3::transports::WebSocket>,
    secret_key: SecretKey,
}

/// Retrieves a private key from a file. The file should contain just the hex-encoded key, nothing else.
fn secret_key_from_file(filename: &Path) -> Result<SecretKey> {
    let key = String::from_utf8(std::fs::read(filename)?)?;
    Ok(SecretKey::from_str(&key[..])?)
}

impl Web3Signer {
    pub async fn new_synced(
        settings: &settings::Settings,
        logger: &slog::Logger,
    ) -> Result<Web3Signer> {
        let node_endpoint = &settings.eth.node_endpoint;
        slog::debug!(logger, "Connecting new web3 client to {}", node_endpoint);
        let web3_client = tokio::time::timeout(Duration::from_secs(5), async {
            Ok(web3::Web3::new(
                web3::transports::WebSocket::new(node_endpoint).await?,
            ))
        })
        // Flatten the Result<Result<>> returned by timeout()
        .map_err(|error| anyhow::Error::new(error))
        .and_then(|x| async { x })
        // Make sure the eth node is fully synced
        .and_then(|web3| async {
            loop {
                match web3.eth().syncing().await? {
                    SyncState::Syncing(info) => {
                        slog::info!(logger, "Waiting for eth node to sync: {:?}", info);
                    }
                    SyncState::NotSyncing => {
                        slog::info!(logger, "Eth node is synced.");
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_secs(4)).await;
            }
            Ok(web3)
        })
        .await?;

        Ok(Self {
            web3: web3_client,
            secret_key: secret_key_from_file(settings.eth.private_key_file.as_path()).expect(
                &format!(
                    "Should read in secret key from: {}",
                    settings.eth.private_key_file.display(),
                ),
            ),
        })
    }

    /// Sign and broadcast a transaction
    async fn sign_and_broadcast<T: Transport>(
        &self,
        tx_details: &ContractCallDetails,
    ) -> Result<H256> {
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

        Ok(tx_hash)
    }
}
