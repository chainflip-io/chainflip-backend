pub mod key_manager;
pub mod stake_manager;

pub mod event_common;

pub mod utils;

use anyhow::{Context, Result};

use pallet_cf_vaults::BlockHeightWindow;
use secp256k1::SecretKey;
use slog::o;
use sp_core::H160;
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};
use web3::{ethabi::Address, types::U64};

use crate::{
    common::Mutex,
    logging::COMPONENT_KEY,
    settings,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};
use futures::{TryFutureExt, TryStreamExt};
use std::{fmt::Debug, fs::read_to_string, str::FromStr, sync::Arc, time::Duration};
use web3::{
    ethabi::{self, Contract, Event},
    signing::{Key, SecretKeyRef},
    transports::WebSocket,
    types::{BlockNumber, Bytes, FilterBuilder, Log, SyncState, TransactionParameters, H256},
    Web3,
};

use tokio_stream::{Stream, StreamExt};

use event_common::EventWithCommon;

use async_trait::async_trait;

#[derive(Error, Debug)]
pub enum EventParseError {
    #[error("Unexpected event signature in log subscription: {0:?}")]
    UnexpectedEvent(H256),
    #[error("Cannot decode missing parameter: '{0}'.")]
    MissingParam(String),
}

// The signature is recalculated on each Event::signature() call, so we use this structure to cache the signture
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

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start_contract_observer<ContractObserver, RPCCLient>(
    contract_observer: ContractObserver,
    web3: &Web3<WebSocket>,
    mut window_receiver: UnboundedReceiver<BlockHeightWindow>,
    state_chain_client: Arc<StateChainClient<RPCCLient>>,
    logger: &slog::Logger,
) where
    ContractObserver: 'static + EthObserver + Sync + Send,
    RPCCLient: 'static + StateChainRpcApi + Sync + Send,
{
    println!("Starting the observer");
    let logger = logger.new(o!(COMPONENT_KEY => "StakeManagerObserver"));
    slog::info!(logger, "Starting");

    let mut option_handle_end_block: Option<(JoinHandle<()>, Arc<Mutex<Option<u64>>>)> = None;

    let contract_observer = Arc::new(contract_observer);

    while let Some(received_window) = window_receiver.recv().await {
        if let Some((handle, end_at_block)) = option_handle_end_block.take() {
            // if we already have a thread, we want to tell it when to stop and await on it
            if let Some(window_to) = received_window.to {
                if let None = *end_at_block.lock().await {
                    // we now have the block we want to end at
                    *end_at_block.lock().await = Some(window_to);
                    handle.await.unwrap();
                }
            } else {
                panic!("Received two 'end' events in a row. This should not occur.");
            }
        } else {
            let task_end_at_block = Arc::new(Mutex::new(received_window.to));

            // clone for capture by tokio task
            let task_end_at_block_c = task_end_at_block.clone();
            let web3 = web3.clone();
            let logger = logger.clone();
            let contract_observer = contract_observer.clone();
            let state_chain_client = state_chain_client.clone();
            option_handle_end_block = Some((
                tokio::spawn(async move {
                    let mut event_stream = contract_observer
                        .event_stream(&web3, received_window.from, &logger)
                        .await
                        .expect("Failed to initialise event stream");

                    // TOOD: Handle None on stream, and result event being an error
                    while let Some(result_event) = event_stream.next().await {
                        let event = result_event.expect("should be valid event type");
                        if let Some(window_to) = *task_end_at_block.lock().await {
                            if event.block_number > window_to {
                                // we have reached the block height we wanted to witness up to
                                // so can stop the witness process
                                break;
                            }
                        }
                        contract_observer
                            .handle_event(event, state_chain_client.clone(), &logger)
                            .await;
                    }
                }),
                task_end_at_block_c,
            ))
        }
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
            web3::transports::WebSocket::new(node_endpoint)
                .await
                .context(here!())?,
        ))
    })
    // Flatten the Result<Result<>> returned by timeout()
    .map_err(anyhow::Error::new)
    .and_then(|x| async { x })
    // Make sure the eth node is fully synced
    .and_then(|web3| async {
        while let SyncState::Syncing(info) = web3
            .eth()
            .syncing()
            .await
            .context("Failure while syncing web3 client")?
        {
            slog::info!(logger, "Waiting for eth node to sync: {:?}", info);
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
        slog::info!(logger, "ETH node is synced.");
        Ok(web3)
    })
    .await
}

/// Enables ETH event streaming via the `Web3` client and signing & broadcasting of txs
#[derive(Clone, Debug)]
pub struct EthBroadcaster {
    web3: Web3<web3::transports::WebSocket>,
    secret_key: SecretKey,
    pub address: Address,
}

impl EthBroadcaster {
    pub fn new(
        settings: &settings::Settings,
        web3: Web3<web3::transports::WebSocket>,
    ) -> Result<Self> {
        let key = read_to_string(settings.eth.private_key_file.as_path())
            .context("Failed to read eth.private_key_file")?;
        let secret_key = SecretKey::from_str(&key[..]).unwrap_or_else(|e| {
            panic!(
                "Should read in secret key from: {}: {}",
                settings.eth.private_key_file.display(),
                e,
            )
        });
        Ok(Self {
            web3,
            secret_key,
            address: SecretKeyRef::new(&secret_key).address(),
        })
    }

    /// Encode and sign a transaction.
    pub async fn encode_and_sign_tx(
        &self,
        unsigned_tx: cf_chains::eth::UnsignedTransaction,
    ) -> Result<Bytes> {
        let tx_params = TransactionParameters {
            to: Some(unsigned_tx.contract),
            data: unsigned_tx.data.into(),
            chain_id: Some(unsigned_tx.chain_id),
            value: unsigned_tx.value,
            transaction_type: Some(web3::types::U64::from(2)),
            ..Default::default()
        };

        Ok(self
            .web3
            .accounts()
            .sign_transaction(tx_params, SecretKeyRef::from(&self.secret_key))
            .await
            .context("Failed to sign ETH transaction")?
            .raw_transaction)
    }

    /// Broadcast a transaction to the network
    pub async fn send(&self, raw_signed_tx: Vec<u8>) -> Result<H256> {
        let tx_hash = self
            .web3
            .eth()
            .send_raw_transaction(raw_signed_tx.into())
            .await
            .context("Failed to send raw signed ETH transaction")?;

        Ok(tx_hash)
    }
}

#[async_trait]
pub trait EthObserver {
    type EventParameters: Debug + Send + Sync + 'static;

    async fn event_stream(
        &self,
        web3: &Web3<WebSocket>,
        from_block: u64,
        logger: &slog::Logger,
    ) -> Result<Box<dyn Stream<Item = Result<EventWithCommon<Self::EventParameters>>> + Unpin + Send>>
    {
        let deployed_address = self.get_deployed_address();
        let decode_log = self.decode_log_closure()?;
        slog::info!(
            logger,
            "Subscribing to Ethereum events from contract at address: {:?}",
            hex::encode(deployed_address)
        );
        // Start future log stream before requesting current block number, to ensure BlockNumber::Pending isn't after current_block
        let future_logs = web3
            .eth_subscribe()
            .subscribe_logs(
                FilterBuilder::default()
                    .from_block(BlockNumber::Latest)
                    .address(vec![deployed_address])
                    .build(),
            )
            .await
            .context("Error subscribing to ETH logs")?;
        let from_block = U64::from(from_block);
        let current_block = web3.eth().block_number().await?;

        // The `fromBlock` parameter doesn't seem to work reliably with subscription streams, so
        // request past block via http and prepend them to the stream manually.
        let (past_logs, exclude_future_logs_before) = if from_block <= current_block {
            (
                web3.eth()
                    .logs(
                        FilterBuilder::default()
                            .from_block(BlockNumber::Number(from_block))
                            .to_block(BlockNumber::Number(current_block))
                            .address(vec![deployed_address])
                            .build(),
                    )
                    .await
                    .context("Failed to fetch past ETH logs")?,
                current_block + 1,
            )
        } else {
            (vec![], from_block)
        };

        let future_logs =
            future_logs
                .map_err(anyhow::Error::new)
                .filter_map(move |result_unparsed_log| {
                    // Need to remove logs that have already been included in past_logs or are before from_block
                    match result_unparsed_log {
                        Ok(Log {
                            block_number: None, ..
                        }) => Some(Err(anyhow::Error::msg("Found log without block number"))),
                        Ok(Log {
                            block_number: Some(block_number),
                            ..
                        }) if block_number < exclude_future_logs_before => None,
                        _ => Some(result_unparsed_log),
                    }
                });

        slog::info!(logger, "Future logs fetched");
        let logger = logger.clone();
        Ok(Box::new(
            tokio_stream::iter(past_logs)
                .map(Ok)
                .chain(future_logs)
                .map(
                    move |result_unparsed_log| -> Result<EventWithCommon<Self::EventParameters>, anyhow::Error> {
                        let result_event = result_unparsed_log
                            .and_then(|log| EventWithCommon::<Self::EventParameters>::decode(&decode_log, log));

                        if let Ok(ok_result) = &result_event {
                            slog::debug!(logger, "Received ETH log {}", ok_result);
                        }

                        result_event
                    },
                ),
        ))
    }

    fn decode_log_closure(
        &self,
    ) -> Result<Box<dyn Fn(H256, ethabi::RawLog) -> Result<Self::EventParameters> + Send>>;

    async fn handle_event<RPCClient>(
        &self,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RPCClient>>,
        logger: &slog::Logger,
    ) where
        RPCClient: 'static + StateChainRpcApi + Sync + Send;

    fn get_deployed_address(&self) -> H160;
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
