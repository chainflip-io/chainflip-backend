mod http_observer;
pub mod key_manager;
pub mod stake_manager;

pub mod event_common;

mod safe_stream;

pub mod utils;

use anyhow::{Context, Result};

use pallet_cf_vaults::BlockHeightWindow;
use secp256k1::SecretKey;
use slog::o;
use sp_core::{H160, U256};
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};
use web3::types::H2048;
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
use std::{fmt::Debug, pin::Pin, str::FromStr, sync::Arc};
use web3::{
    ethabi::{self, Contract, Event},
    signing::{Key, SecretKeyRef},
    types::{BlockNumber, Bytes, FilterBuilder, SyncState, TransactionParameters, H256},
    Web3,
};

use tokio_stream::{Stream, StreamExt};

use event_common::EventWithCommon;

use async_trait::async_trait;

pub trait BlockHeaderable {
    fn hash(&self) -> Option<H256>;

    fn logs_bloom(&self) -> Option<H2048>;

    fn number(&self) -> Option<U64>;
}

impl BlockHeaderable for web3::types::BlockHeader {
    fn hash(&self) -> Option<H256> {
        self.hash
    }

    fn logs_bloom(&self) -> Option<H2048> {
        Some(self.logs_bloom)
    }

    fn number(&self) -> Option<U64> {
        self.number
    }
}

impl<TX> BlockHeaderable for web3::types::Block<TX> {
    fn hash(&self) -> Option<H256> {
        self.hash
    }

    fn logs_bloom(&self) -> Option<H2048> {
        self.logs_bloom
    }

    fn number(&self) -> Option<U64> {
        self.number
    }
}

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
pub async fn start_contract_observer<ContractObserver, RpcClient, EthWsRpc>(
    contract_observer: ContractObserver,
    eth_rpc: &EthWsRpc,
    mut window_receiver: UnboundedReceiver<BlockHeightWindow>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    logger: &slog::Logger,
) where
    ContractObserver: 'static + EthObserver + Sync + Send,
    RpcClient: 'static + StateChainRpcApi + Sync + Send,
    EthWsRpc: 'static + EthWsRpcApi + Sync + Send + Clone,
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

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

    async fn chain_id(&self) -> Result<U256>;
}

#[async_trait]
pub trait EthWsRpcApi: EthRpcApi {
    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>>;
}

#[async_trait]
pub trait EthHttpRpcApi: EthRpcApi {
    async fn block_number(&self) -> Result<U64>;
}

/// Wraps the web3 library, so can use a trait to make testing easier
#[derive(Clone)]
pub struct EthWsRpcClient {
    web3: Web3<web3::transports::WebSocket>,
}

impl EthWsRpcClient {
    pub async fn new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
        let node_endpoint = &eth_settings.node_endpoint;
        slog::debug!(logger, "Connecting new web3 client to {}", node_endpoint);
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
impl EthRpcApi for EthWsRpcClient {
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

#[async_trait]
impl EthWsRpcApi for EthWsRpcClient {
    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>> {
        Ok(self.web3.eth_subscribe().subscribe_new_heads().await?)
    }
}

/// Wraps the web3 library, so can use a trait to make testing easier
#[derive(Clone)]
pub struct EthHttpRpcClient {
    web3: Web3<web3::transports::Http>,
}

#[async_trait]
impl EthRpcApi for EthHttpRpcClient {
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

#[async_trait]
impl EthHttpRpcApi for EthHttpRpcClient {
    async fn block_number(&self) -> Result<U64> {
        Ok(self.web3.eth().block_number().await?)
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

    async fn ws_event_stream<BlockHeaderStream, EthRpc>(
        &self,
        from_block: u64,
        deployed_address: H160,
        safe_head_stream: BlockHeaderStream,
        decode_log: Box<dyn Fn(H256, ethabi::RawLog) -> Result<Self::EventParameters> + Send>,
        eth_rpc: &EthRpc,
        logger: &slog::Logger,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<EventWithCommon<Self::EventParameters>>> + Unpin + Send>>,
    >
    where
        BlockHeaderStream: Stream<Item = BlockHeader> + 'static + Send,
        EthRpc: 'static + EthRpcApi + Send + Sync + Clone,
    {
        let from_block = U64::from(from_block);
        let mut safe_head_stream = Box::pin(safe_head_stream);
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
                    Box::pin(
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

    /// Get an event stream for the contract, returning the stream only if the head of the stream is
    /// ahead of from_block (otherwise it will wait until we have reached from_block)
    async fn event_stream<EthWsRpc: 'static + EthWsRpcApi + Send + Sync + Clone>(
        &self,
        eth_rpc: &EthWsRpc,
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

        let safe_head_stream = safe_eth_log_header_stream(eth_head_stream, ETH_BLOCK_SAFETY_MARGIN);

        let ws_stream = self
            .ws_event_stream(
                from_block,
                deployed_address,
                safe_head_stream,
                decode_log,
                eth_rpc,
                logger,
            )
            .await;

        // we get events from the ws stream

        Err(anyhow::Error::msg("NO stream, RIP"))
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
}
