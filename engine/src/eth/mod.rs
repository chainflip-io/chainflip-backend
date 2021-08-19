pub mod key_manager;
pub mod stake_manager;

mod eth_event_streamer;

pub mod eth_broadcaster;
pub mod eth_tx_encoding;
pub mod utils;

use anyhow::Result;

use web3::ethabi::{Contract, Event};
use web3::types::SyncState;
use thiserror::Error;

use std::time::Duration;
use crate::settings;
use web3::types::H256;
use web3;

/// The `Error` type for errors specific to this module.
#[derive(Error, Debug)]
pub enum EventProducerError {
    #[error("Unexpected event signature in log subscription: {0:#}")]
    UnexpectedEvent(H256),

    /// A log was received with an empty "topics" vector, shouldn't happen.
    #[error("Expected log to contain topics, got empty vector.")]
    EmptyTopics,

    /// Tried to decode a parameter that doesn't exist in the log.
    #[error("Cannot decode missing parameter: '{0}'.")]
    MissingParam(String),
}

// The signature is recalculated on each Event::signature() call, so we use this structure to cache the signture-
pub struct SignatureAndEvent {
    pub signature : H256,
    pub event : Event
}
impl SignatureAndEvent {
    pub fn new(contract : &Contract, name : &str) -> Result<Self> {
        let event = contract.event(name)?;
        Ok(Self {signature : event.signature(), event : event.clone()})
    }
}

pub async fn new_web3_client(settings : &settings::Settings, logger : &slog::Logger) -> Result<web3::Web3<web3::transports::WebSocket>> {
    let node_endpoint = &settings.eth.node_endpoint;
    slog::debug!(
        logger,
        "Connecting new web3 client to {}",
        node_endpoint
    );
    match tokio::time::timeout(
        Duration::from_secs(5),
        async {
            Ok(web3::Web3::new(web3::transports::WebSocket::new(node_endpoint).await?))
        },
    ).await {
        Ok(result) => match result {
            Ok(web3) => {
                // Make sure the eth node is fully synced
                loop {
                    match web3.eth().syncing().await? {
                        SyncState::Syncing(info) => {
                            slog::info!(logger, "Waiting for eth node to sync: {:?}", info);
                        }
                        SyncState::NotSyncing => {
                            slog::info!(
                                logger,
                                "Eth node is synced."
                            );
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(4)).await;
                };
                Ok(web3)
            },
            Err(error) => Err(error)
        },
        Err(_) => {
            // Connection timeout
            Err(anyhow::Error::msg(format!(
                "Timeout connecting to {:?}",
                node_endpoint
            )))
        }
    }

}