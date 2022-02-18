mod http_observer;
pub mod key_manager;
pub mod stake_manager;

pub mod event_common;

mod safe_stream;

pub mod utils;

use anyhow::{Context, Result};

use futures::stream::{self, repeat, select, Repeat, Select, Zip};
use pallet_cf_vaults::BlockHeightWindow;
use secp256k1::SecretKey;
use slog::o;
use sp_core::{H160, U256};
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};
use web3::types::{Block, H2048};
use web3::{
    api::SubscriptionStream,
    ethabi::Address,
    types::{BlockHeader, CallRequest, Filter, Log, SignedTransaction, U64},
};

use crate::constants::{ETH_FALLING_BEHIND_MARGIN_BLOCKS, ETH_NUMBER_OF_BLOCK_BEFORE_LOG_BEHIND};
use crate::eth::http_observer::{polling_http_head_stream, HTTP_POLL_INTERVAL};
use crate::logging::{ETH_HTTP_STREAM_RETURNED, ETH_STREAM_BEHIND, ETH_WS_STREAM_RETURNED};
use crate::{
    common::{read_clean_and_decode_hex_str_file, Mutex},
    constants::{ETH_BLOCK_SAFETY_MARGIN, ETH_NODE_CONNECTION_TIMEOUT, SYNC_POLL_INTERVAL},
    eth::safe_stream::{filtered_log_stream_by_contract, safe_eth_log_header_stream},
    logging::COMPONENT_KEY,
    settings,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};
use futures::TryFutureExt;
use std::collections::HashMap;
use std::fmt;
use std::{fmt::Debug, pin::Pin, str::FromStr, sync::Arc};
use web3::{
    ethabi::{self, Contract, Event},
    signing::{Key, SecretKeyRef},
    types::{BlockNumber, Bytes, FilterBuilder, SyncState, TransactionParameters, H256},
    Web3,
};

use futures::StreamExt;

use tokio_stream::Stream;

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
pub async fn start_contract_observer<ContractObserver, RpcClient, EthWsRpc, EthHttpRpc>(
    contract_observer: ContractObserver,
    eth_ws_rpc: &EthWsRpc,
    eth_http_rpc: &EthHttpRpc,
    mut window_receiver: UnboundedReceiver<BlockHeightWindow>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    logger: &slog::Logger,
) where
    ContractObserver: 'static + EthObserver + Sync + Send,
    RpcClient: 'static + StateChainRpcApi + Sync + Send,
    EthWsRpc: 'static + EthWsRpcApi + Sync + Send + Clone,
    EthHttpRpc: 'static + EthHttpRpcApi + Sync + Send + Clone,
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
            let eth_ws_rpc = eth_ws_rpc.clone();
            let eth_http_rpc = eth_http_rpc.clone();
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
                        .event_stream(&eth_http_rpc, &eth_ws_rpc, received_window.from, &logger)
                        .await
                        .expect("Failed to initialise event stream");

                    // TOOD: Handle None on stream, and result event being an error
                    while let Some(event) = event_stream.next().await {
                        if let Some(window_to) = *task_end_at_block.lock().await {
                            // TODO: Have the stream end when the safe head gets to the block number,
                            // not just when we receive an event (which could be arbitrarily far in the future)
                            // past our window_to
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

    async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>>;
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
            .context("Failed to estimate gas with WS Client")
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
            .context("Failed to sign transaction with WS Client")
    }

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
        self.web3
            .eth()
            .send_raw_transaction(rlp)
            .await
            .context("Failed to send raw transaction with WS Client")
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        self.web3
            .eth()
            .logs(filter)
            .await
            .context("Failed to fetch ETH logs with WS Client")
    }

    async fn chain_id(&self) -> Result<U256> {
        self.web3
            .eth()
            .chain_id()
            .await
            .context("Failed to fetch ETH ChainId with WS Client")
    }
}

#[async_trait]
impl EthWsRpcApi for EthWsRpcClient {
    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>> {
        self.web3
            .eth_subscribe()
            .subscribe_new_heads()
            .await
            .context("Failed to subscribe to new heads with WS Client")
    }
}

#[derive(Clone)]
pub struct EthHttpRpcClient {
    web3: Web3<web3::transports::Http>,
}

impl EthHttpRpcClient {
    pub fn new(eth_settings: &settings::Eth) -> Result<Self> {
        let node_endpoint = &eth_settings.node_endpoint;
        let http = web3::transports::Http::new(node_endpoint)
            .context("Failed to create HTTP Transport for web3 client")?;
        let web3 = web3::Web3::new(http);

        Ok(Self { web3 })
    }
}

#[async_trait]
impl EthRpcApi for EthHttpRpcClient {
    async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256> {
        self.web3
            .eth()
            .estimate_gas(req, block)
            .await
            .context("Failed to estimate gas with HTTP client")
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
            .context("Failed to sign transaction with HTTP client")
    }

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
        self.web3
            .eth()
            .send_raw_transaction(rlp)
            .await
            .context("Failed to send raw transaction with HTTP client")
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        self.web3
            .eth()
            .logs(filter)
            .await
            .context("Failed to fetch ETH logs with HTTP client")
    }

    async fn chain_id(&self) -> Result<U256> {
        Ok(self.web3.eth().chain_id().await?)
    }
}

#[async_trait]
impl EthHttpRpcApi for EthHttpRpcClient {
    async fn block_number(&self) -> Result<U64> {
        self.web3
            .eth()
            .block_number()
            .await
            .context("Failed to fetch block number with HTTP client")
    }

    async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>> {
        self.web3
            .eth()
            .block(block_number.into())
            .await
            .context("Failed to fetch block with HTTP client")
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

        slog::debug!(
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

// Used to zip on the streams, so we know which stream is returning
#[derive(Clone, PartialEq)]
pub enum TransportProtocol {
    Http,
    Ws,
}

impl fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TransportProtocol::Ws => write!(f, "Websocket"),
            TransportProtocol::Http => write!(f, "Http"),
        }
    }
}

// Specify a default type for the mock
#[async_trait]
pub trait EthObserver {
    type EventParameters: Debug + Send + Sync + 'static;

    /// Takes a stream of BlockHeaderable items, and turns this into a stream of logs/events
    /// for all logs/events from a particular contract
    async fn log_stream_from_head_stream<BlockHeaderStream, EthRpc, EthBlockHeader>(
        &self,
        from_block: u64,
        contract_address: H160,
        safe_head_stream: BlockHeaderStream,
        eth_rpc: &EthRpc,
        logger: &slog::Logger,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<EventWithCommon<Self::EventParameters>>> + Unpin + Send>>,
    >
    where
        BlockHeaderStream: Stream<Item = EthBlockHeader> + 'static + Send,
        EthRpc: 'static + EthRpcApi + Send + Sync + Clone,
        EthBlockHeader: BlockHeaderable + Send + Sync + Clone + 'static,
    {
        let from_block = U64::from(from_block);
        let mut safe_head_stream = Box::pin(safe_head_stream);
        let decode_log = self.decode_log_closure()?;
        // only allow pulling from the stream once we are actually at our from_block number
        while let Some(current_best_safe_block_header) = safe_head_stream.next().await {
            let best_safe_block_number = current_best_safe_block_header
                .number()
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
                            .address(vec![contract_address])
                            .build(),
                    )
                    .await
                    .context("Failed to fetch past ETH logs")?;

                let future_logs = filtered_log_stream_by_contract(
                    safe_head_stream,
                    eth_rpc.clone(),
                    contract_address,
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
    async fn event_stream<EthWsRpc, EthHttpRpc>(
        &self,
        eth_http_rpc: &EthHttpRpc,
        eth_ws_rpc: &EthWsRpc,
        // usually the start of the validator's active window
        from_block: u64,
        logger: &slog::Logger,
    ) -> Result<Pin<Box<dyn Stream<Item = EventWithCommon<Self::EventParameters>> + Send>>>
    where
        EthWsRpc: 'static + EthWsRpcApi + Send + Sync + Clone,
        EthHttpRpc: 'static + EthHttpRpcApi + Send + Sync + Clone,
    {
        println!("Running event stream");
        let deployed_address = self.get_contract_address();
        slog::info!(
            logger,
            "Subscribing to Ethereum events from contract at address: {:?}",
            hex::encode(deployed_address)
        );

        let eth_head_stream = eth_ws_rpc.subscribe_new_heads().await?;

        let safe_ws_head_stream =
            safe_eth_log_header_stream(eth_head_stream, ETH_BLOCK_SAFETY_MARGIN);

        let safe_ws_event_logs = self
            .log_stream_from_head_stream(
                from_block,
                deployed_address,
                safe_ws_head_stream,
                eth_ws_rpc,
                logger,
            )
            .await?;

        let safe_http_head_stream =
            polling_http_head_stream(eth_http_rpc.clone(), HTTP_POLL_INTERVAL).await;

        let safe_http_event_logs = self
            .log_stream_from_head_stream(
                from_block,
                deployed_address,
                safe_http_head_stream,
                eth_http_rpc,
                logger,
            )
            .await?;

        self.merged_log_stream(safe_ws_event_logs, safe_http_event_logs, logger.clone())
            .await
    }

    /// Takes two *safe* streams, i.e. ones that progress at a maximum of a block at a time
    /// therefore it does not need to handle reorgs itself. It will panic if a reorg or fast-forward
    /// (likely due to a sync issue) is detected in either of the streams
    async fn merged_log_stream(
        &self,
        safe_ws_log_stream: Pin<
            Box<dyn Stream<Item = Result<EventWithCommon<Self::EventParameters>>> + Unpin + Send>,
        >,
        safe_http_log_stream: Pin<
            Box<dyn Stream<Item = Result<EventWithCommon<Self::EventParameters>>> + Unpin + Send>,
        >,
        logger: slog::Logger,
    ) -> Result<Pin<Box<dyn Stream<Item = EventWithCommon<Self::EventParameters>> + Send>>> {
        let logger = logger.new(o!(COMPONENT_KEY => "MergedETHStream"));
        let safe_ws_log_stream = safe_ws_log_stream.zip(repeat(TransportProtocol::Ws));
        let safe_http_log_stream = safe_http_log_stream.zip(repeat(TransportProtocol::Http));

        let merged_stream = select(safe_ws_log_stream, safe_http_log_stream);

        type SingleStreamLogs<EventParameters> = Zip<
            Pin<Box<dyn Stream<Item = Result<EventWithCommon<EventParameters>>> + Unpin + Send>>,
            Repeat<TransportProtocol>,
        >;
        struct StreamState<EventParameters: Debug + Send + Sync + 'static> {
            merged_stream:
                Select<SingleStreamLogs<EventParameters>, SingleStreamLogs<EventParameters>>,
            last_yielded_block_number: u64,
            last_http_block_pulled: u64,
            last_ws_block_pulled: u64,
            txs_in_current_block: HashMap<(H256, U256), ()>,
            logger: slog::Logger,
        }

        let init_data = StreamState::<Self::EventParameters> {
            merged_stream,
            last_yielded_block_number: 0,
            last_http_block_pulled: 0,
            last_ws_block_pulled: 0,
            txs_in_current_block: HashMap::new(),
            logger,
        };

        fn log_stream_returned_and_behind<EventParameters: std::fmt::Debug + Send + Sync>(
            state: &StreamState<EventParameters>,
            protocol: TransportProtocol,
            yield_item: &EventWithCommon<EventParameters>,
        ) {
            // Do the necessary logging
            match protocol {
                TransportProtocol::Http => {
                    slog::info!(
                        state.logger,
                        #ETH_HTTP_STREAM_RETURNED,
                        "Processing ETH log {} from {} stream",
                        yield_item,
                        protocol
                    );
                    if state.last_ws_block_pulled + ETH_FALLING_BEHIND_MARGIN_BLOCKS
                        <= yield_item.block_number
                    {
                        let blocks_behind = yield_item.block_number - state.last_ws_block_pulled;
                        if !(state.last_ws_block_pulled == 0)
                            && (blocks_behind % ETH_NUMBER_OF_BLOCK_BEFORE_LOG_BEHIND == 0)
                        {
                            slog::warn!(
                                state.logger,
                                #ETH_STREAM_BEHIND,
                                "HTTP stream at ETH block {} but Websocket stream at ETH block {}",
                                yield_item.block_number,
                                state.last_ws_block_pulled,
                            );
                        }
                    }
                }
                TransportProtocol::Ws => {
                    slog::info!(
                        state.logger,
                        #ETH_WS_STREAM_RETURNED,
                        "Processing ETH log {} from {} stream",
                        yield_item,
                        protocol
                    );
                    if state.last_http_block_pulled + ETH_FALLING_BEHIND_MARGIN_BLOCKS
                        <= yield_item.block_number
                    {
                        let blocks_behind = yield_item.block_number - state.last_http_block_pulled;
                        if !(state.last_http_block_pulled == 0)
                            && (blocks_behind % ETH_NUMBER_OF_BLOCK_BEFORE_LOG_BEHIND == 0)
                        {
                            slog::warn!(
                                state.logger,
                                #ETH_STREAM_BEHIND,
                                "Websocket stream at ETH block {} but HTTP stream at ETH block {}",
                                yield_item.block_number,
                                state.last_http_block_pulled,
                            );
                        }
                    }
                }
            }
        }

        Ok(Box::pin(stream::unfold(
            init_data,
            move |mut state| async move {
                // we can get multiple events for the same block number. So we cannot use block number here
                // to determine if we have returned or not.
                // Instead we have to do something a little more sophisticated, tracking hashes for events
                // and clearing the hash cache as we progress for each block
                if let Some((yield_item, protocol, state)) = loop {
                    if let Some((current_item, protocol)) = state.merged_stream.next().await {
                        let current_item = current_item.unwrap();
                        let current_item_block_number = current_item.block_number;
                        let current_item_tx_hash = current_item.tx_hash;
                        let current_item_log_index = current_item.log_index;

                        match protocol {
                            // this function takes safe streams, so we should only progress
                            // forward by one block at a time
                            TransportProtocol::Http => {
                                assert_eq!(
                                    state.last_http_block_pulled + 1,
                                    current_item_block_number
                                );
                                state.last_http_block_pulled = current_item_block_number
                            }
                            TransportProtocol::Ws => {
                                assert_eq!(
                                    state.last_ws_block_pulled + 1,
                                    current_item_block_number
                                );
                                state.last_ws_block_pulled = current_item_block_number
                            }
                        };

                        if (current_item_block_number > state.last_yielded_block_number)
                        // first iteration
                        || state.last_yielded_block_number == 0
                        {
                            // we've progressed, so we can clear our log cache for the previous block
                            state.txs_in_current_block = HashMap::new();

                            state.last_yielded_block_number = current_item_block_number;
                            state
                                .txs_in_current_block
                                .insert((current_item_tx_hash, current_item_log_index), ());
                            break Some((current_item, protocol, state));
                        } else if current_item_block_number == state.last_yielded_block_number {
                            let log_already_yielded = state
                                .txs_in_current_block
                                .insert((current_item_tx_hash, current_item_log_index), ())
                                .is_some();

                            // if the key already existed, we have already emitted it
                            if !log_already_yielded {
                                break Some((current_item, protocol, state));
                            }
                        } else {
                            // discard anything that's less than the last yielded block number
                            // since we've already returned anything we need to from those logs
                            slog::debug!(
                            state.logger,
                            "Already returned logs from this block number. Discarding ETH log {} from {} stream",
                            current_item,
                            protocol
                        );
                            continue;
                        }
                    } else {
                        break None;
                    }
                } {
                    log_stream_returned_and_behind(&state, protocol, &yield_item);
                    Some((yield_item, state))
                } else {
                    None
                }
            },
        )))
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

    fn get_contract_address(&self) -> H160;
}

/// Events that both the Key and Stake Manager contracts can output (Shared.sol)
#[derive(Debug, PartialEq)]
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
pub mod mocks {
    use super::*;

    use mockall::mock;

    // Create a mock of the http interface of web3
    mock! {
        // MockEthHttpRpc will be the name of the mock
        pub EthHttpRpc {}

        impl Clone for EthHttpRpc {
            fn clone(&self) -> Self;
        }

        #[async_trait]
        impl EthRpcApi for EthHttpRpc {
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
        impl EthHttpRpcApi for EthHttpRpc {
            async fn block_number(&self) -> Result<U64>;

            async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>>;
        }
    }

    // Create a mock of the Websockets interface of web3
    mock! {
        // MockEthWsRpc will be the name of the mock
        pub EthWsRpc {}

        impl Clone for EthWsRpc {
            fn clone(&self) -> Self;
        }

        #[async_trait]
        impl EthRpcApi for EthWsRpc {
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
        impl EthWsRpcApi for EthWsRpc {
            async fn subscribe_new_heads(
                &self,
            ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>>;
        }
    }
}

#[cfg(test)]
mod merged_stream_tests {
    use std::time::Duration;

    use crate::logging::test_utils::new_test_logger;
    use crate::logging::test_utils::new_test_logger_with_tag_cache;
    use crate::logging::ETH_WS_STREAM_RETURNED;

    use super::key_manager::ChainflipKey;
    use super::key_manager::KeyManagerEvent;

    use super::key_manager::KeyManager;

    use super::*;

    fn test_km_contract() -> KeyManager {
        KeyManager::new(H160::default()).unwrap()
    }

    fn key_change(block_number: u64, log_index: u8) -> Result<EventWithCommon<KeyManagerEvent>> {
        Ok(EventWithCommon::<KeyManagerEvent> {
            tx_hash: Default::default(),
            log_index: U256::from(log_index),
            block_number,
            event_parameters: KeyManagerEvent::KeyChange {
                signed: true,
                old_key: ChainflipKey::default(),
                new_key: ChainflipKey::default(),
            },
        })
    }

    // Generate a stream for each protocol, that, when selected upon, will return
    // in the order the items are passed in
    // This is useful to test more "real world" scenarios, as stream::iter will always
    // immediately yield, therefore items will always be pealed off the streams
    // alternatingly
    fn interleaved_streams(
        // contains the streams in the order they will return
        items: Vec<(Result<EventWithCommon<KeyManagerEvent>>, TransportProtocol)>,
    ) -> (
        // ws
        impl Stream<Item = Result<EventWithCommon<KeyManagerEvent>>>,
        // http
        impl Stream<Item = Result<EventWithCommon<KeyManagerEvent>>>,
    ) {
        assert!(items.len() > 0, "should have at least one item");

        const DELAY_DURATION_MILLIS: u64 = 10;

        let mut protocol_last_returned = items.first().unwrap().1.clone();
        let mut http_items = Vec::new();
        let mut ws_items = Vec::new();
        let mut total_delay_increment = 0;

        for (item, protocol) in items {
            // if we are returning the same, we can just go the next, since we are ordered
            let delay = Duration::from_millis(if protocol == protocol_last_returned {
                0
            } else {
                total_delay_increment = total_delay_increment + DELAY_DURATION_MILLIS;
                total_delay_increment
            });

            match protocol {
                TransportProtocol::Http => http_items.push((item, delay)),
                TransportProtocol::Ws => ws_items.push((item, delay)),
            };

            protocol_last_returned = protocol;
        }

        let delayed_stream = |items: Vec<(Result<EventWithCommon<KeyManagerEvent>>, Duration)>| {
            let items = items.into_iter();
            Box::pin(stream::unfold(items, |mut items| async move {
                while let Some((i, d)) = items.next() {
                    tokio::time::sleep(d).await;
                    return Some((i, items));
                }
                None
            }))
        };

        (delayed_stream(ws_items), delayed_stream(http_items))
    }

    #[tokio::test]
    async fn empty_inners_return_none() {
        // use concrete type for tests
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let safe_ws_log_stream = Box::pin(stream::empty());
        let safe_http_log_stream = Box::pin(stream::empty());

        let mut stream = key_manager
            .merged_log_stream(safe_ws_log_stream, safe_http_log_stream, logger)
            .await
            .unwrap();

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_does_not_return_duplicate_events() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let key_change_1 = 10;
        let key_change_2 = 15;

        let safe_ws_log_stream = Box::pin(stream::iter([
            key_change(key_change_1, 0),
            key_change(key_change_2, 0),
        ]));
        let safe_http_log_stream = Box::pin(stream::iter([
            key_change(key_change_1, 0),
            key_change(key_change_2, 0),
        ]));

        let mut stream = key_manager
            .merged_log_stream(safe_ws_log_stream, safe_http_log_stream, logger)
            .await
            .unwrap();

        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_1, 0).unwrap()
        );
        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_2, 0).unwrap()
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_handles_broken_stream() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        // websockets is down :(
        let safe_ws_log_stream = Box::pin(stream::empty());
        // http is working
        let safe_http_log_stream = Box::pin(stream::iter([
            key_change(8, 0),
            key_change(10, 0),
            key_change(12, 0),
        ]));

        let mut stream = key_manager
            .merged_log_stream(safe_ws_log_stream, safe_http_log_stream, logger)
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(8, 0).unwrap());
        assert_eq!(stream.next().await.unwrap(), key_change(10, 0).unwrap());
        assert_eq!(stream.next().await.unwrap(), key_change(12, 0).unwrap());
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_handles_behind_stream() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let safe_ws_log_stream = Box::pin(stream::iter([
            key_change(10, 0),
            key_change(12, 0),
            key_change(14, 0),
        ]));
        // is 2 blocks behind the ws stream
        let safe_http_log_stream = Box::pin(stream::iter([
            key_change(8, 0),
            key_change(10, 0),
            key_change(12, 0),
        ]));

        let mut stream = key_manager
            .merged_log_stream(safe_ws_log_stream, safe_http_log_stream, logger)
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(10, 0).unwrap());
        assert_eq!(stream.next().await.unwrap(), key_change(12, 0).unwrap());
        assert_eq!(stream.next().await.unwrap(), key_change(14, 0).unwrap());
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_handles_logs_in_same_tx() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let safe_ws_log_stream = Box::pin(stream::iter([
            key_change(10, 0),
            key_change(10, 1),
            key_change(14, 0),
        ]));

        let safe_http_log_stream = Box::pin(stream::iter([
            key_change(10, 0),
            key_change(10, 1),
            key_change(14, 0),
        ]));

        let mut stream = key_manager
            .merged_log_stream(safe_ws_log_stream, safe_http_log_stream, logger)
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(10, 0).unwrap());
        assert_eq!(stream.next().await.unwrap(), key_change(10, 1).unwrap());
        assert_eq!(stream.next().await.unwrap(), key_change(14, 0).unwrap());
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn interleaved_streams_works_as_expected() {
        let mut items = Vec::new();
        // return
        items.push((key_change(10, 0), TransportProtocol::Ws));
        // return
        items.push((key_change(11, 0), TransportProtocol::Ws));
        // ignore
        items.push((key_change(10, 0), TransportProtocol::Http));
        // ignore
        items.push((key_change(11, 0), TransportProtocol::Http));
        // return
        items.push((key_change(12, 0), TransportProtocol::Http));
        // ignore
        items.push((key_change(12, 0), TransportProtocol::Ws));
        // return
        items.push((key_change(13, 0), TransportProtocol::Http));
        // ignore
        items.push((key_change(13, 0), TransportProtocol::Ws));
        // return
        items.push((key_change(14, 0), TransportProtocol::Ws));

        let (logger, mut tag_cache) = new_test_logger_with_tag_cache();
        let (ws_stream, http_stream) = interleaved_streams(items);

        let key_manager = test_km_contract();
        let mut merged_stream = key_manager
            .merged_log_stream(Box::pin(ws_stream), Box::pin(http_stream), logger)
            .await
            .unwrap();

        merged_stream.next().await;
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
        tag_cache.clear();

        merged_stream.next().await;
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
        tag_cache.clear();

        merged_stream.next().await;
        assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_RETURNED));
        tag_cache.clear();

        merged_stream.next().await;
        assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_RETURNED));
        tag_cache.clear();

        merged_stream.next().await;
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
        tag_cache.clear();
    }

    #[tokio::test]
    async fn merged_stream_notifies_once_every_x_blocks_when_one_falls_behind() {
        let key_manager = test_km_contract();
        let (logger, tag_cache) = new_test_logger_with_tag_cache();

        let ws_range = 10..54;
        let events = ws_range.clone().map(|i| key_change(i, 0));
        let safe_ws_log_stream = Box::pin(stream::iter(events));

        let safe_http_log_stream = Box::pin(stream::iter([key_change(10, 0)]));

        let mut merged_stream = key_manager
            .merged_log_stream(
                Box::pin(safe_ws_log_stream),
                Box::pin(safe_http_log_stream),
                logger,
            )
            .await
            .unwrap();

        for i in ws_range {
            let event = merged_stream.next().await.unwrap();
            assert_eq!(event, key_change(i, 0).unwrap());
        }

        assert_eq!(tag_cache.get_tag_count(ETH_STREAM_BEHIND), 4);
        assert!(merged_stream.next().await.is_none());
    }

    // TODO:
    // #[tokio::test]
    // async fn merged_stream_panics_on_bad_input_streams() {
    //     let key_manager = test_km_contract();
    //     let logger = new_test_logger();

    //     let safe_ws_log_stream = Box::pin(stream::iter([
    //         key_change(10, 0),
    //         key_change(10, 02),
    //         key_change(14, 0),
    //     ]));
    //     // is 2 blocks behind the ws stream
    //     let safe_http_log_stream = Box::pin(stream::iter([
    //         key_change(8, 0),
    //         key_change(10, 0),
    //         key_change(12, 0),
    //     ]));

    //     let mut stream = key_manager
    //         .merged_log_stream(safe_ws_log_stream, safe_http_log_stream, logger)
    //         .await
    //         .unwrap();

    //     assert_eq!(stream.next().await.unwrap(), key_change(10, 0).unwrap());
    //     assert_eq!(stream.next().await.unwrap(), key_change(12, 0).unwrap());
    //     assert_eq!(stream.next().await.unwrap(), key_change(14, 0).unwrap());
    //     assert!(stream.next().await.is_none());
    // }
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
