pub mod key_manager;
pub mod stake_manager;

pub mod eth_event_streamer;

pub mod utils;

use anyhow::Result;

use secp256k1::SecretKey;
use thiserror::Error;

use crate::settings;
use futures::TryFutureExt;
use std::fs::read_to_string;
use std::str::FromStr;
use std::time::Duration;
use web3::{
    ethabi::{Contract, Event},
    signing::SecretKeyRef,
    types::{SyncState, TransactionParameters, H160, H256},
    Web3,
};

#[derive(Error, Debug)]
pub enum EventParseError {
    #[error("Unexpected event signature in log subscription: {0:?}")]
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

pub async fn new_synced_web3_client(
    settings: &settings::Settings,
    logger: &slog::Logger,
) -> Result<Web3<web3::transports::WebSocket>> {
    let node_endpoint = &settings.eth.node_endpoint;
    slog::debug!(logger, "Connecting new web3 client to {}", node_endpoint);
    tokio::time::timeout(Duration::from_secs(5), async {
        Ok(web3::Web3::new(
            web3::transports::WebSocket::new(node_endpoint).await?,
        ))
    })
    // Flatten the Result<Result<>> returned by timeout()
    .map_err(|error| anyhow::Error::new(error))
    .and_then(|x| async { x })
    // Make sure the eth node is fully synced
    .and_then(|web3| async {
        while let SyncState::Syncing(info) = web3.eth().syncing().await? {
            slog::info!(logger, "Waiting for eth node to sync: {:?}", info);
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
        slog::info!(logger, "Eth node is synced.");
        Ok(web3)
    })
    .await
}

/// Enables ETH event streaming via the `Web3` client and signing & broadcasting of txs
#[derive(Clone, Debug)]
pub struct EthBroadcaster {
    web3: Web3<web3::transports::WebSocket>,
    secret_key: SecretKey,
}

impl EthBroadcaster {
    pub fn new(
        settings: &settings::Settings,
        web3: Web3<web3::transports::WebSocket>,
    ) -> Result<Self> {
        let key = read_to_string(settings.eth.private_key_file.as_path())?;
        Ok(Self {
            web3,
            secret_key: SecretKey::from_str(&key[..]).expect(&format!(
                "Should read in secret key from: {}",
                settings.eth.private_key_file.display(),
            )),
        })
    }

    /// Sign and broadcast a transaction to a particular contract
    pub async fn send(&self, tx_data: Vec<u8>, contract: H160) -> Result<H256> {
        let tx_params = TransactionParameters {
            to: Some(contract),
            data: tx_data.into(),
            ..Default::default()
        };

        let raw_transaction = self
            .web3
            .accounts()
            .sign_transaction(tx_params, SecretKeyRef::from(&self.secret_key))
            .await?
            .raw_transaction;

        let tx_hash = self
            .web3
            .eth()
            .send_raw_transaction(raw_transaction)
            .await?;

        Ok(tx_hash)
    }
}
