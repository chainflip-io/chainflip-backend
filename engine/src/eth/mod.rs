pub mod key_manager;
pub mod stake_manager;

pub mod event_common;

mod safe_stream;

pub mod utils;

use anyhow::{Context, Result};

use pallet_cf_vaults::BlockHeightWindow;
use regex::Regex;
use secp256k1::SecretKey;
use slog::o;
use sp_core::{H160, U256};
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};
use web3::{
    api::SubscriptionStream,
    ethabi::Address,
    types::{BlockHeader, CallRequest, Filter, Log, SignedTransaction, U64},
};

use crate::{
    common::{read_clean_and_decode_hex_str_file, Mutex},
    constants::{ETH_BLOCK_SAFETY_MARGIN, ETH_NODE_CONNECTION_TIMEOUT, SYNC_POLL_INTERVAL},
    eth::safe_stream::{filtered_log_stream_by_contract, safe_eth_log_header_stream},
    logging::COMPONENT_KEY,
    settings,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};
use futures::TryFutureExt;
use std::{fmt::Debug, str::FromStr, sync::Arc};
use web3::{
    ethabi::{self, Contract, Event},
    signing::{Key, SecretKeyRef},
    types::{BlockNumber, Bytes, FilterBuilder, SyncState, TransactionParameters, H256},
    Web3,
};

use tokio_stream::{Stream, StreamExt};

use event_common::EventWithCommon;

use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

// TODO: Not possible to fix the clippy warning here. At the moment we
// need to ignore it on a global level.
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

// TODO: Look at refactoring this to take specific "start" and "end" blocks, rather than this being implicit over the windows
// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start_contract_observer<ContractObserver, RpcClient, EthRpc>(
    contract_observer: ContractObserver,
    eth_rpc: &EthRpc,
    mut window_receiver: UnboundedReceiver<BlockHeightWindow>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    logger: &slog::Logger,
) where
    ContractObserver: 'static + EthObserver + Sync + Send,
    RpcClient: 'static + StateChainRpcApi + Sync + Send,
    EthRpc: 'static + EthRpcApi + Sync + Send + Clone,
{
    let logger = logger.new(o!(COMPONENT_KEY => "EthObserver"));
    slog::info!(logger, "Starting");

    type TaskEndBlock = Arc<Mutex<Option<u64>>>;

    let mut option_handle_end_block: Option<(JoinHandle<()>, TaskEndBlock)> = None;

    let contract_observer = Arc::new(contract_observer);

    while let Some(received_window) = window_receiver.recv().await {
        if let Some((handle, end_at_block)) = option_handle_end_block.take() {
            // if we already have a thread, we want to tell it when to stop and await on it
            if let Some(window_to) = received_window.to {
                if end_at_block.lock().await.is_none() {
                    // we now have the block we want to end at
                    *end_at_block.lock().await = Some(window_to);
                    handle.await.unwrap();
                }
            } else {
                // NB: If we receive another start event, then we just keep the current task going
                panic!("Received two 'end' events in a row. This should not occur.");
            }
        } else {
            let task_end_at_block = Arc::new(Mutex::new(received_window.to));

            // clone for capture by tokio task
            let task_end_at_block_c = task_end_at_block.clone();
            let eth_rpc = eth_rpc.clone();
            let logger = logger.clone();
            let contract_observer = contract_observer.clone();
            let state_chain_client = state_chain_client.clone();
            option_handle_end_block = Some((
                tokio::spawn(async move {
                    slog::info!(
                        logger,
                        "Start observing from ETH block: {}",
                        received_window.from
                    );
                    let mut event_stream = contract_observer
                        .event_stream(&eth_rpc, received_window.from, &logger)
                        .await
                        .expect("Failed to initialise event stream");

                    // TOOD: Handle None on stream, and result event being an error
                    while let Some(result_event) = event_stream.next().await {
                        let event = result_event.expect("should be valid event type");
                        if let Some(window_to) = *task_end_at_block.lock().await {
                            if event.block_number > window_to {
                                slog::info!(
                                    logger,
                                    "Finished observing events at ETH block: {}",
                                    event.block_number
                                );
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

#[cfg_attr(test, automock)]
#[async_trait]
pub trait EthRpcApi {
    async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256>;

    async fn sign_transaction(
        &self,
        tx: TransactionParameters,
        key: &SecretKey,
    ) -> Result<SignedTransaction>;

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256>;

    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>>;

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

    async fn chain_id(&self) -> Result<U256>;
}

/// Wraps the web3 library, so can use a trait to make testing easier
#[derive(Clone)]
pub struct EthRpcClient {
    web3: Web3<web3::transports::WebSocket>,
}

impl EthRpcClient {
    pub async fn new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
        let node_endpoint = &eth_settings.node_endpoint;
        match redact_secret_eth_node_endpoint(node_endpoint) {
            Ok(redacted) => {
                slog::trace!(logger, "Connecting new web3 client to {}", redacted);
            }
            Err(e) => {
                slog::error!(logger, "Could not redact secret from node endpoint: {}", e);
                slog::trace!(logger, "Connecting new web3 client");
            }
        }
        let web3 = tokio::time::timeout(ETH_NODE_CONNECTION_TIMEOUT, async {
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
                .context("Failure while syncing EthRpcClient client")?
            {
                slog::info!(
                    logger,
                    "Waiting for ETH node to sync. Sync state is: {:?}. Checking again in {:?} ...",
                    info,
                    SYNC_POLL_INTERVAL
                );
                tokio::time::sleep(SYNC_POLL_INTERVAL).await;
            }
            slog::info!(logger, "ETH node is synced.");
            Ok(web3)
        })
        .await?;

        Ok(Self { web3 })
    }
}

#[async_trait]
impl EthRpcApi for EthRpcClient {
    async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256> {
        self.web3
            .eth()
            .estimate_gas(req, block)
            .await
            .context("Failed to estimate gas")
    }

    async fn sign_transaction(
        &self,
        tx: TransactionParameters,
        key: &SecretKey,
    ) -> Result<SignedTransaction> {
        self.web3
            .accounts()
            .sign_transaction(tx, SecretKeyRef::from(key))
            .await
            .context("Failed to sign transaction")
    }

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
        self.web3
            .eth()
            .send_raw_transaction(rlp)
            .await
            .context("Failed to send raw transaction")
    }

    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>> {
        Ok(self.web3.eth_subscribe().subscribe_new_heads().await?)
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        self.web3
            .eth()
            .logs(filter)
            .await
            .context("Failed to fetch ETH logs")
    }

    async fn chain_id(&self) -> Result<U256> {
        Ok(self.web3.eth().chain_id().await?)
    }
}

/// Enables ETH event streaming via the `Web3` client and signing & broadcasting of txs
#[derive(Clone, Debug)]
pub struct EthBroadcaster<EthRpc: EthRpcApi> {
    eth_rpc: EthRpc,
    secret_key: SecretKey,
    pub address: Address,
    logger: slog::Logger,
}

impl<EthRpc: EthRpcApi> EthBroadcaster<EthRpc> {
    pub fn new(
        eth_settings: &settings::Eth,
        eth_rpc: EthRpc,
        logger: &slog::Logger,
    ) -> Result<Self> {
        let secret_key = read_clean_and_decode_hex_str_file(
            &eth_settings.private_key_file,
            "Ethereum Private Key",
            |key| SecretKey::from_str(key).map_err(anyhow::Error::new),
        )?;
        Ok(Self {
            eth_rpc,
            secret_key,
            address: SecretKeyRef::new(&secret_key).address(),
            logger: logger.new(o!(COMPONENT_KEY => "EthBroadcaster")),
        })
    }

    #[cfg(test)]
    pub fn new_test(eth_rpc: EthRpc, logger: &slog::Logger) -> Self {
        // just a fake key
        let secret_key =
            SecretKey::from_str("000000000000000000000000000000000000000000000000000000000000aaaa")
                .unwrap();
        Self {
            eth_rpc,
            secret_key,
            address: SecretKeyRef::new(&secret_key).address(),
            logger: logger.new(o!(COMPONENT_KEY => "EthBroadcaster")),
        }
    }

    /// Encode and sign a transaction.
    pub async fn encode_and_sign_tx(
        &self,
        unsigned_tx: cf_chains::eth::UnsignedTransaction,
    ) -> Result<Bytes> {
        let mut tx_params = TransactionParameters {
            to: Some(unsigned_tx.contract),
            data: unsigned_tx.data.clone().into(),
            chain_id: Some(unsigned_tx.chain_id),
            value: unsigned_tx.value,
            transaction_type: Some(web3::types::U64::from(2)),
            // Set the gas really high (~half gas in a block) for the estimate, since the estimation call requires you to
            // input at least as much gas as the estimate will return (stupid? yes)
            gas: U256::from(15_000_000),
            ..Default::default()
        };
        // query for the gas estimate if the SC didn't provide it
        let gas_estimate = if let Some(gas_limit) = unsigned_tx.gas_limit {
            gas_limit
        } else {
            let call_request: CallRequest = tx_params.clone().into();
            self.eth_rpc
                .estimate_gas(call_request, None)
                .await
                .context("Failed to estimate gas")?
        };

        // increase the estimate by 50%
        let uint256_2 = U256::from(2);
        tx_params.gas = gas_estimate
            .saturating_mul(uint256_2)
            .saturating_sub(gas_estimate.checked_div(uint256_2).unwrap());

        slog::trace!(
            self.logger,
            "Gas estimate for unsigned tx: {:?} is {}. Setting 50% higher at: {}",
            unsigned_tx,
            gas_estimate,
            tx_params.gas
        );

        Ok(self
            .eth_rpc
            .sign_transaction(tx_params, &self.secret_key)
            .await
            .context("Failed to sign ETH transaction")?
            .raw_transaction)
    }

    /// Broadcast a transaction to the network
    pub async fn send(&self, raw_signed_tx: Vec<u8>) -> Result<H256> {
        self.eth_rpc
            .send_raw_transaction(raw_signed_tx.into())
            .await
    }
}
#[async_trait]
pub trait EthObserver {
    type EventParameters: Debug + Send + Sync + 'static;

    /// Get an event stream for the contract, returning the stream only if the head of the stream is
    /// ahead of from_block (otherwise it will wait until we have reached from_block)
    async fn event_stream<EthRpc: 'static + EthRpcApi + Send + Sync + Clone>(
        &self,
        eth_rpc: &EthRpc,
        // usually the start of the validator's active window
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

        let eth_head_stream = eth_rpc.subscribe_new_heads().await?;

        let mut safe_head_stream =
            safe_eth_log_header_stream(eth_head_stream, ETH_BLOCK_SAFETY_MARGIN);

        let from_block = U64::from(from_block);
        // only allow pulling from the stream once we are actually at our from_block number
        while let Some(current_best_safe_block_header) = safe_head_stream.next().await {
            let best_safe_block_number = current_best_safe_block_header
                .number
                .expect("Should have block number");
            // we only want to start observing once we reach the from_block specified
            if best_safe_block_number < from_block {
                slog::trace!(
                    logger,
                    "Not witnessing until ETH block `{}` Received block `{}` from stream.",
                    from_block,
                    best_safe_block_number
                );
                continue;
            } else {
                // our chain_head is above the from_block number
                // The `fromBlock` parameter doesn't seem to work reliably with the web3 subscription streams
                let past_logs = eth_rpc
                    .get_logs(
                        FilterBuilder::default()
                            // from_block and to_block are *inclusive*
                            .from_block(BlockNumber::Number(from_block))
                            .to_block(BlockNumber::Number(best_safe_block_number))
                            .address(vec![deployed_address])
                            .build(),
                    )
                    .await
                    .context("Failed to fetch past ETH logs")?;

                let future_logs = filtered_log_stream_by_contract(
                    safe_head_stream,
                    eth_rpc.clone(),
                    deployed_address,
                    logger.clone(),
                )
                .await;

                let logger = logger.clone();
                return Ok(
                    Box::new(
                        tokio_stream::iter(past_logs).chain(future_logs).map(
                            move |unparsed_log| -> Result<
                                EventWithCommon<Self::EventParameters>,
                                anyhow::Error,
                            > {
                                let result_event = EventWithCommon::<Self::EventParameters>::decode(
                                    &decode_log,
                                    unparsed_log,
                                );
                                if let Ok(ok_result) = &result_event {
                                    slog::debug!(logger, "Received ETH event log {}", ok_result);
                                }
                                result_event
                            },
                        ),
                    ),
                );
            }
        }
        Err(anyhow::Error::msg("No events in safe head stream"))
    }

    fn decode_log_closure(
        &self,
    ) -> Result<Box<dyn Fn(H256, ethabi::RawLog) -> Result<Self::EventParameters> + Send>>;

    async fn handle_event<RpcClient>(
        &self,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        logger: &slog::Logger,
    ) where
        RpcClient: 'static + StateChainRpcApi + Sync + Send;

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

const MAX_SECRET_CHARACTERS_REVEALED: usize = 3;
const SCHEMA_PADDING_LEN: usize = 3;

/// Partially redacts the secret in the url of the node endpoint.
///  eg: `wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/` -> `wss://cdc****.rinkeby.ws.rivet.cloud/`
fn redact_secret_eth_node_endpoint(endpoint: &str) -> Result<String> {
    let re = Regex::new(r"[0-9a-fA-F]{32}").unwrap();
    if re.is_match(endpoint) {
        // A 32 character hex string was found, redact it
        let mut endpoint_redacted = endpoint.to_string();
        for capture in re.captures_iter(endpoint) {
            endpoint_redacted = endpoint_redacted.replace(
                &capture[0],
                &format!(
                    "{}****",
                    &capture[0]
                        .split_at(capture[0].len().min(MAX_SECRET_CHARACTERS_REVEALED))
                        .0
                ),
            );
        }
        Ok(endpoint_redacted)
    } else {
        // No secret was found, so just redact almost all of the url
        let url = url::Url::parse(endpoint)
            .map_err(anyhow::Error::msg)
            .with_context(|| "Failed to parse node endpoint into a URL")?;
        Ok(format!(
            "{}****",
            endpoint
                .split_at(usize::min(
                    url.scheme().len() + SCHEMA_PADDING_LEN + MAX_SECRET_CHARACTERS_REVEALED,
                    endpoint.len()
                ))
                .0
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::logging::test_utils::new_test_logger;

    use super::*;

    #[test]
    fn cfg_test_create_eth_broadcaster_works() {
        let eth_rpc_api_mock = MockEthRpcApi::new();
        let logger = new_test_logger();
        EthBroadcaster::new_test(eth_rpc_api_mock, &logger);
    }

    #[test]
    fn test_secret_web_addresses() {
        assert_eq!(
            redact_secret_eth_node_endpoint(
                "wss://mainnet.infura.io/ws/v3/d52c362116b640b98a166d08d3170a42"
            )
            .unwrap(),
            "wss://mainnet.infura.io/ws/v3/d52****"
        );
        assert_eq!(
            redact_secret_eth_node_endpoint(
                "wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/"
            )
            .unwrap(),
            "wss://cdc****.rinkeby.ws.rivet.cloud/"
        );
        assert_eq!(
            redact_secret_eth_node_endpoint("wss://non_32hex_secret.rinkeby.ws.rivet.cloud/")
                .unwrap(),
            "wss://non****"
        );
        assert_eq!(
            redact_secret_eth_node_endpoint("wss://a").unwrap(),
            "wss://a****"
        );
        assert!(redact_secret_eth_node_endpoint("no.schema.com").is_err());
    }
}
