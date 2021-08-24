pub mod key_manager;
pub mod stake_manager;

mod eth_event_streamer;

pub mod eth_broadcaster;
pub mod utils;

use anyhow::Result;

use thiserror::Error;
use web3::ethabi::{Contract, Event};
use web3::types::SyncState;

use crate::settings;
use futures::TryFutureExt;
use std::time::Duration;
use web3;
use web3::types::H256;

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

pub async fn new_synced_web3_client(
    settings: &settings::Settings,
    logger: &slog::Logger,
) -> Result<web3::Web3<web3::transports::WebSocket>> {
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
    .await
}
