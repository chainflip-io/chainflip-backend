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
    ethabi::{self, Contract, Event},
    signing::SecretKeyRef,
    types::{Bytes, SyncState, TransactionParameters, H160, H256},
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
    .map_err(anyhow::Error::new)
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
            secret_key: SecretKey::from_str(&key[..]).unwrap_or_else(|e| {
                panic!(
                    "Should read in secret key from: {}: {}",
                    settings.eth.private_key_file.display(),
                    e,
                )
            }),
        })
    }

    pub async fn sign_tx(&self, unsigned_tx: cf_chains::eth::UnsignedTransaction) -> Result<Bytes> {
        let tx_params = TransactionParameters {
            to: Some(unsigned_tx.contract),
            data: unsigned_tx.data.into(),
            chain_id: Some(unsigned_tx.chain_id),
            value: unsigned_tx.value,
            ..Default::default()
        };

        Ok(self
            .web3
            .accounts()
            .sign_transaction(tx_params, SecretKeyRef::from(&self.secret_key))
            .await?
            .raw_transaction)
    }

    /// Broadcast a transaction to the network
    pub async fn send(&self, raw_signed_tx: Vec<u8>) -> Result<H256> {
        let tx_hash = self
            .web3
            .eth()
            .send_raw_transaction(raw_signed_tx.into())
            .await?;

        Ok(tx_hash)
    }
}

/// Events that both the Key and Stake Manager contracts can output (Shared.sol)
#[derive(Debug)]
pub enum SharedEvent {
    /// `Refunded(amount)`
    Refunded {
        /// The amount of ETH refunded
        amount: u128,
    },

    /// `RefundFailed(to, amount, currentBalance)`
    RefundFailed {
        /// The refund recipient
        to: ethabi::Address,
        /// The amount of ETH to refund
        amount: u128,
        /// The contract's current balance
        current_balance: u128,
    },
}

fn decode_shared_event_closure(
    contract: &Contract,
) -> Result<impl Fn(H256, ethabi::RawLog) -> Result<SharedEvent>> {
    let refunded = SignatureAndEvent::new(contract, "Refunded")?;
    let refund_failed = SignatureAndEvent::new(contract, "RefundFailed")?;

    Ok(
        move |signature: H256, raw_log: ethabi::RawLog| -> Result<SharedEvent> {
            if signature == refunded.signature {
                let log = refunded.event.parse_log(raw_log)?;
                Ok(SharedEvent::Refunded {
                    amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                })
            } else if signature == refund_failed.signature {
                let log = refund_failed.event.parse_log(raw_log)?;
                Ok(SharedEvent::RefundFailed {
                    to: utils::decode_log_param::<ethabi::Address>(&log, "to")?,
                    amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                    current_balance: utils::decode_log_param::<ethabi::Uint>(
                        &log,
                        "currentBalance",
                    )?
                    .as_u128(),
                })
            } else {
                Err(anyhow::Error::from(EventParseError::UnexpectedEvent(
                    signature,
                )))
            }
        },
    )
}
