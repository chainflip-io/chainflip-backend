mod http_safe_stream;
pub mod key_manager;
pub mod stake_manager;

pub mod event_common;

mod ws_safe_stream;

pub mod utils;

use anyhow::{Context, Result};

use crate::constants::{ETH_FALLING_BEHIND_MARGIN_BLOCKS, ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL};
use crate::eth::http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL};
use crate::eth::ws_safe_stream::safe_ws_head_stream;
use crate::logging::{
    ETH_HTTP_STREAM_RETURNED, ETH_STREAM_BEHIND, ETH_WS_STREAM_RETURNED,
    SAFE_PROTOCOL_STREAM_JUMP_BACK,
};
use crate::{
    common::{read_clean_and_decode_hex_str_file, Mutex},
    constants::{
        ETH_BLOCK_SAFETY_MARGIN, ETH_NODE_CONNECTION_TIMEOUT, SYNC_POLL_INTERVAL,
        WEB3_REQUEST_TIMEOUT,
    },
    logging::COMPONENT_KEY,
    settings,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};
use ethbloom::{Bloom, Input};
use futures::stream::{self, repeat, select, select_all, Fuse, Repeat, Select, Zip};
use futures::TryFutureExt;
use pallet_cf_vaults::BlockHeightWindow;
use secp256k1::SecretKey;
use slog::o;
use sp_core::{H160, U256};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::{fmt::Debug, pin::Pin, str::FromStr, sync::Arc};
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};
use web3::types::{Block, FilterBuilder, H2048};
use web3::{
    api::SubscriptionStream,
    ethabi::Address,
    types::{BlockHeader, CallRequest, Filter, Log, SignedTransaction, U64},
};
use web3::{
    ethabi::{self, Contract, Event},
    signing::{Key, SecretKeyRef},
    types::{BlockNumber, Bytes, SyncState, TransactionParameters, H256},
    Web3,
};

use futures::StreamExt;

use tokio_stream::Stream;

use event_common::EventWithCommon;

use async_trait::async_trait;

#[derive(Debug, PartialEq)]
pub struct CFEthBlockHeader {
    pub block_number: U64,
    pub logs_bloom: H2048,
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
pub async fn start_contract_observer<ContractObserver, StateChainRpc, EthWsRpc, EthHttpRpc>(
    contract_observer: ContractObserver,
    eth_ws_rpc: &EthWsRpc,
    eth_http_rpc: &EthHttpRpc,
    mut window_receiver: UnboundedReceiver<BlockHeightWindow>,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    logger: &slog::Logger,
) where
    ContractObserver: 'static + EthObserver + Sync + Send,
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
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
                        .event_stream(
                            eth_http_rpc,
                            eth_ws_rpc,
                            received_window.from,
                            logger.clone(),
                        )
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

    async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>>;
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
        let ws_node_endpoint = &eth_settings.ws_node_endpoint;
        slog::debug!(logger, "Connecting new web3 client to {}", ws_node_endpoint);
        let web3 = tokio::time::timeout(ETH_NODE_CONNECTION_TIMEOUT, async {
            Ok(web3::Web3::new(
                web3::transports::WebSocket::new(ws_node_endpoint)
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
        let request_fut = self.web3.eth().logs(filter);

        // NOTE: if this does time out we will most likely have a
        // "memory leak" associated with rust-web3's state for this
        // request not getting properly cleaned up
        tokio::time::timeout(WEB3_REQUEST_TIMEOUT, request_fut)
            .await
            .context("Web3 WS get_logs request timeout")?
            .context("Failed to fetch ETH logs with WS client")
    }

    async fn chain_id(&self) -> Result<U256> {
        self.web3
            .eth()
            .chain_id()
            .await
            .context("Failed to fetch ETH ChainId with WS Client")
    }

    async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>> {
        self.web3
            .eth()
            .block(block_number.into())
            .await
            .context("Failed to fetch block with HTTP client")
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
        let web3 = web3::Web3::new(
            web3::transports::Http::new(&eth_settings.http_node_endpoint)
                .context("Failed to create HTTP Transport for web3 client")?,
        );

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
        let request_fut = self.web3.eth().logs(filter);

        // NOTE: if this does time out we will most likely have a
        // "memory leak" associated with rust-web3's state for this
        // request not getting properly cleaned up
        tokio::time::timeout(WEB3_REQUEST_TIMEOUT, request_fut)
            .await
            .context("Web3 HTTP get_logs request timeout")?
            .context("Failed to fetch ETH logs with HTTP client")
    }

    async fn chain_id(&self) -> Result<U256> {
        Ok(self.web3.eth().chain_id().await?)
    }

    async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>> {
        self.web3
            .eth()
            .block(block_number.into())
            .await
            .context("Failed to fetch block with HTTP client")
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

#[derive(Debug)]
pub struct BlockEvents<EventParameters: Debug> {
    pub block_number: u64,
    pub events: Option<Vec<EventWithCommon<EventParameters>>>,
}

// Specify a default type for the mock
#[async_trait]
pub trait EthObserver {
    type EventParameters: Debug + Send + Sync + 'static;

    async fn get_logs_for_block<EthRpc>(
        &self,
        header: CFEthBlockHeader,
        eth_rpc: EthRpc,
        contract_address: H160,
        logger: slog::Logger,
    ) -> Result<BlockEvents<Self::EventParameters>>
    where
        EthRpc: EthRpcApi + Clone + 'static + Send + Sync,
    {
        let block_number = header.block_number;
        let mut contract_bloom = Bloom::default();
        contract_bloom.accrue(Input::Raw(&contract_address.0));
        let decode_log_fn = self.decode_log_closure().unwrap();

        // if we have logs for this block, fetch them.
        if header.logs_bloom.contains_bloom(&contract_bloom) {
            match eth_rpc
                .get_logs(
                    FilterBuilder::default()
                        .from_block(BlockNumber::Number(block_number))
                        .to_block(BlockNumber::Number(block_number))
                        .address(vec![contract_address])
                        .build(),
                )
                .await
            {
                Ok(logs) => {
                    match logs
                        .into_iter()
                        .map(
                            move |unparsed_log| -> Result<
                                EventWithCommon<Self::EventParameters>,
                                anyhow::Error,
                            > {
                                EventWithCommon::<Self::EventParameters>::decode(
                                    &decode_log_fn,
                                    unparsed_log,
                                )
                            },
                        )
                        .collect::<Result<Vec<_>>>()
                    {
                        Ok(events) => Ok(BlockEvents {
                            block_number: block_number.as_u64(),
                            events: Some(events),
                        }),
                        Err(err) => {
                            slog::error!(
                                logger,
                                "Failed to decode ETH logs for block `{}`: {}",
                                block_number,
                                err,
                            );
                            Err(err)
                        }
                    }
                }
                Err(err) => {
                    slog::error!(
                        logger,
                        "Failed to request ETH logs for block `{}`: {}",
                        block_number,
                        err,
                    );
                    // we expected there to be logs, but failed to fetch them
                    Err(err)
                }
            }
        } else {
            // we didn't expect there to be logs, so didn't fetch
            Ok(BlockEvents {
                block_number: block_number.as_u64(),
                events: None,
            })
        }
    }

    /// Takes a head stream and turns it into a stream of BlockEvents for consumption by the merged stream
    async fn block_logs_stream_from_head_stream<'a, BlockHeaderStream, EthRpc>(
        &'a self,
        from_block: u64,
        contract_address: H160,
        safe_head_stream: BlockHeaderStream,
        eth_rpc: EthRpc,
        logger: slog::Logger,
    ) -> Result<Pin<Box<dyn 'a + Stream<Item = Result<BlockEvents<Self::EventParameters>>> + Send>>>
    where
        BlockHeaderStream: Stream<Item = Result<CFEthBlockHeader>> + 'static + Send,
        EthRpc: 'static + EthRpcApi + Send + Sync + Clone,
    {
        let from_block = U64::from(from_block);
        let mut safe_head_stream = Box::pin(safe_head_stream);

        // only allow pulling from the stream once we are actually at our from_block number
        while let Some(current_best_safe_block_header) = safe_head_stream.next().await {
            let best_safe_block_header = current_best_safe_block_header
                .context("Could not get first safe ETH block header")?;
            let best_safe_block_number = best_safe_block_header.block_number;
            // we only want to start observing once we reach the from_block specified
            if best_safe_block_number < from_block {
                slog::trace!(
                    logger,
                    "Not witnessing until ETH block `{}` Received block `{}` from stream.",
                    from_block,
                    best_safe_block_number
                );
            } else {
                // our chain_head is above the from_block number
                // The `fromBlock` parameter doesn't seem to work reliably with the web3 subscription streams

                let eth_rpc_c = eth_rpc.clone();

                // block log stream for contract instead of this.
                let past_logs = stream::iter(from_block.as_u64()..=best_safe_block_number.as_u64())
                    .then(move |block_number| {
                        let eth_rpc = eth_rpc_c.clone();
                        // TODO: Can this async block be removed, by using .then()?
                        async move {
                            eth_rpc
                                .block(U64::from(block_number))
                                .await
                                .and_then(|opt_block| {
                                    opt_block.ok_or(anyhow::Error::msg(
                                        "Could not find ETH block in HTTP safe stream",
                                    ))
                                })
                        }
                    });

                // if the block doesn't contain shit, we say the *BLOCK* is an error. we have not yet fetched the logs
                let past_logs = past_logs.then(|block| async {
                        block.and_then(|block| {
                            if block.number.is_none() || block.logs_bloom.is_none() {
                                Err(anyhow::Error::msg(
                                    "HTTP block header did not contain necessary block number and/or logs bloom",
                                ))
                            } else {
                                Ok(CFEthBlockHeader {
                                    block_number: block.number.unwrap(),
                                    logs_bloom: block.logs_bloom.unwrap(),
                                })
                            }
                        })
                    });

                // TODO: Revise this comment
                // This will return a stream of BlockEvents, it will return for every block
                // If the header is error, then it returns an error
                // If the bloom says nothing interesting is in the block, logs = None
                // If the bloom is interesting, and we fail to fetch logs. logs = Some(Err)
                // If the bloom is interesting and we fetch the logs. logs = Some(Ok)
                return Ok(Box::pin(past_logs.chain(safe_head_stream).then(
                    move |result_header| {
                        let eth_rpc = eth_rpc.clone();
                        let logger = logger.clone();
                        async move {
                            self.get_logs_for_block(
                                result_header?,
                                eth_rpc,
                                contract_address,
                                logger,
                            )
                            .await
                        }
                    },
                )));
            }
        }
        Err(anyhow::Error::msg("No events in safe head stream"))
    }

    /// Get an event stream for the contract, returning the stream only if the head of the stream is
    /// ahead of from_block (otherwise it will wait until we have reached from_block)
    async fn event_stream<'a, EthWsRpc, EthHttpRpc>(
        &'a self,
        eth_http_rpc: EthHttpRpc,
        eth_ws_rpc: EthWsRpc,
        // usually the start of the validator's active window
        from_block: u64,
        // Look at borrowing here
        logger: slog::Logger,
        // This stream must be Send, so it can be used by the spawn
    ) -> Result<Pin<Box<dyn 'a + Stream<Item = EventWithCommon<Self::EventParameters>> + Send>>>
    where
        EthWsRpc: 'static + EthWsRpcApi + Send + Sync + Clone,
        EthHttpRpc: 'static + EthHttpRpcApi + Send + Sync + Clone,
    {
        let deployed_address = self.get_contract_address();
        slog::info!(
            logger,
            "Subscribing to Ethereum events from contract at address: {:?}",
            hex::encode(deployed_address)
        );

        let eth_head_stream = eth_ws_rpc.subscribe_new_heads().await?;

        let safe_ws_head_stream = safe_ws_head_stream(eth_head_stream, ETH_BLOCK_SAFETY_MARGIN);

        let safe_ws_block_events = self
            .block_logs_stream_from_head_stream(
                from_block,
                deployed_address,
                safe_ws_head_stream,
                eth_ws_rpc,
                logger.clone(),
            )
            .await?;

        let safe_http_head_stream =
            safe_polling_http_head_stream(eth_http_rpc.clone(), HTTP_POLL_INTERVAL, logger.clone())
                .await;

        let safe_http_block_events = self
            .block_logs_stream_from_head_stream(
                from_block,
                deployed_address,
                safe_http_head_stream,
                eth_http_rpc,
                logger.clone(),
            )
            .await?;

        self.merged_block_events_stream(
            safe_ws_block_events,
            safe_http_block_events,
            logger.clone(),
        )
        .await
    }

    async fn merged_block_events_stream<'a, BlockEventsStreamWs, BlockEventsStreamHttp>(
        &'a self,
        safe_ws_block_events_stream: BlockEventsStreamWs,
        safe_http_block_events_stream: BlockEventsStreamHttp,
        logger: slog::Logger,
    ) -> Result<Pin<Box<dyn 'a + Stream<Item = EventWithCommon<Self::EventParameters>> + Send>>>
    where
        BlockEventsStreamWs:
            'a + Stream<Item = Result<BlockEvents<Self::EventParameters>>> + Unpin + Send,
        BlockEventsStreamHttp:
            'a + Stream<Item = Result<BlockEvents<Self::EventParameters>>> + Unpin + Send,
    {
        struct ProtocolState {
            last_block_pulled: u64,
            protocol: TransportProtocol,
        }

        struct MergedStreamState<EventParameters: Debug> {
            last_block_yielded: u64,
            logger: slog::Logger,
            events_to_yield: VecDeque<EventWithCommon<EventParameters>>,
        }

        struct StreamState<
            BlockEventsStreamWs: Stream,
            BlockEventsStreamHttp: Stream,
            EventParameters: Debug,
        > {
            ws_state: ProtocolState,
            ws_stream: Fuse<BlockEventsStreamWs>,
            http_state: ProtocolState,
            http_stream: Fuse<BlockEventsStreamHttp>,
            merged_stream_state: MergedStreamState<EventParameters>,
        }

        let init_state =
            StreamState::<BlockEventsStreamWs, BlockEventsStreamHttp, Self::EventParameters> {
                ws_state: ProtocolState {
                    last_block_pulled: 0,
                    protocol: TransportProtocol::Ws,
                },
                ws_stream: safe_ws_block_events_stream.fuse(),
                http_state: ProtocolState {
                    last_block_pulled: 0,
                    protocol: TransportProtocol::Http,
                },
                http_stream: safe_http_block_events_stream.fuse(),
                merged_stream_state: MergedStreamState {
                    last_block_yielded: 0,
                    events_to_yield: VecDeque::new(),
                    logger,
                },
            };

        fn log_when_stream_behind<EventParameters: Debug>(
            protocol_state: &ProtocolState,
            other_protocol_state: &ProtocolState,
            merged_stream_state: &MergedStreamState<EventParameters>,
            // the current block events
            block_events: &BlockEvents<EventParameters>,
        ) {
            match protocol_state.protocol {
                TransportProtocol::Http => {
                    slog::info!(
                        merged_stream_state.logger,
                        #ETH_HTTP_STREAM_RETURNED,
                        "ETH block {} returning from {} stream",
                        block_events.block_number,
                        protocol_state.protocol
                    );
                }
                TransportProtocol::Ws => {
                    slog::info!(
                        merged_stream_state.logger,
                        #ETH_WS_STREAM_RETURNED,
                        "ETH block {} returning from {} stream",
                        block_events.block_number,
                        protocol_state.protocol
                    );
                }
            }

            println!(
                "Last block yielded: {}, last block pulled: {}",
                merged_stream_state.last_block_yielded, other_protocol_state.last_block_pulled
            );
            let blocks_behind =
                merged_stream_state.last_block_yielded - other_protocol_state.last_block_pulled;

            if ((other_protocol_state.last_block_pulled + ETH_FALLING_BEHIND_MARGIN_BLOCKS)
                <= block_events.block_number)
                && (blocks_behind % ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL == 0)
            {
                slog::warn!(
                    merged_stream_state.logger,
                    #ETH_STREAM_BEHIND,
                    "{} stream at ETH block {} but {} stream at ETH block {}",
                    protocol_state.protocol,
                    block_events.block_number,
                    other_protocol_state.protocol,
                    protocol_state.last_block_pulled,
                );
            }
        }

        async fn catch_up_other_stream<
            EventParameters: Debug,
            BlockEventsStream: Stream<Item = Result<BlockEvents<EventParameters>>> + Unpin,
        >(
            protocol_state: &mut ProtocolState,
            mut protocol_stream: BlockEventsStream,
            next_block_to_yield: u64,
            logger: &slog::Logger,
        ) -> Result<BlockEvents<EventParameters>> {
            // we want a pointer to inside the stream state items, not the state itself.
            while let Some(result_block_events) = protocol_stream.next().await {
                if let Ok(block_events) = result_block_events {
                    protocol_state.last_block_pulled = block_events.block_number;
                    if protocol_state.last_block_pulled == next_block_to_yield {
                        return Ok(block_events);
                    } else if protocol_state.last_block_pulled < next_block_to_yield {
                        slog::trace!(logger, "ETH {} stream pulled block {} but still below the next block to yield of {}", protocol_state.protocol, block_events.block_number, next_block_to_yield)
                    } else {
                        return Err(anyhow::Error::msg(
                            "The stream has skipped blocks. We were expecting a contiguous sequence of blocks",
                        ));
                    }
                } else {
                    return Err(anyhow::Error::msg(
                        "The {} stream has failed after pulling block: {} while attempting to ",
                    ));
                }
            }
            Err(anyhow::Error::msg(format!(
                "The {} stream failed to yield any values.",
                protocol_state.protocol,
            )))
        }

        /// Returns a block only if we are ready to yield this particular block
        async fn do_for_protocol<
            BlockEventsStream: Stream<Item = Result<BlockEvents<EventParameters>>> + Unpin,
            EventParameters: Debug,
        >(
            merged_stream_state: &mut MergedStreamState<EventParameters>,
            protocol_state: &mut ProtocolState,
            other_protocol_state: &mut ProtocolState,
            other_protocol_stream: BlockEventsStream,
            result_block_events: Result<BlockEvents<EventParameters>>,
        ) -> Result<Option<BlockEvents<EventParameters>>> {
            println!(
                "doing for protocol: with block events: {:?}",
                result_block_events
            );
            let next_block_to_yield = merged_stream_state.last_block_yielded + 1;

            if let Ok(block_events) = result_block_events {
                protocol_state.last_block_pulled = block_events.block_number;

                if block_events.block_number > merged_stream_state.last_block_yielded {
                    log_when_stream_behind(
                        protocol_state,
                        other_protocol_state,
                        merged_stream_state,
                        &block_events,
                    );
                    merged_stream_state.last_block_yielded = block_events.block_number;
                    Ok(Some(block_events))
                } else {
                    slog::trace!(
                        merged_stream_state.logger,
                        "Ignoring log from {}, already produced by other stream",
                        block_events.block_number
                    );
                    Ok(None)
                }
            } else {
                // we got an error block.

                let we_yielded_last_block =
                    protocol_state.last_block_pulled == merged_stream_state.last_block_yielded;

                // if we yieled the last one, we let the other stream progress to try and cover for us
                if we_yielded_last_block {
                    Ok(Some(
                        catch_up_other_stream(
                            other_protocol_state,
                            other_protocol_stream,
                            next_block_to_yield,
                            &merged_stream_state.logger,
                        )
                        .await?,
                    ))
                } else {
                    // we're behind, so we just log an error and continue on.
                    slog::error!(
                        merged_stream_state.logger,
                        "Failed to fetch block from {} stream. Expecting to successfully get block for {}",
                        protocol_state.protocol,
                        next_block_to_yield
                    );
                    Ok(None)
                }
            }
        }

        let stream = stream::unfold(init_state, |mut stream_state| async move {
            loop {
                if let Some(event_to_yield) =
                    stream_state.merged_stream_state.events_to_yield.pop_front()
                {
                    break Some((event_to_yield, stream_state));
                }

                let iter_result;
                tokio::select! {
                    Some(result_block_events) = stream_state.ws_stream.next() => {
                        iter_result = do_for_protocol(&mut stream_state.merged_stream_state, &mut stream_state.ws_state, &mut stream_state.http_state, &mut stream_state.http_stream, result_block_events).await;

                    }
                    Some(result_block_events) = stream_state.http_stream.next() => {
                        iter_result = do_for_protocol(&mut stream_state.merged_stream_state, &mut stream_state.http_state, &mut stream_state.ws_state, &mut stream_state.ws_stream, result_block_events).await;
                    }
                    else => break None
                }

                match iter_result {
                    Ok(opt_block_events) => {
                        if let Some(block_events) = opt_block_events {
                            if let Some(events) = block_events.events {
                                stream_state.merged_stream_state.events_to_yield =
                                    events.into_iter().collect();
                            } else {
                                slog::debug!(
                                    stream_state.merged_stream_state.logger,
                                    "No interesting events in ETH block {}",
                                    block_events.block_number
                                );
                            }
                        }
                    }
                    Err(err) => {
                        slog::error!(
                            stream_state.merged_stream_state.logger,
                            "Error in ETH merged event stream: {}",
                            err
                        );
                        break None;
                    }
                }
            }
        });

        Ok(Box::pin(stream))
    }

    // Could we have a closure that decodes the whole thing?
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

            async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>>;
        }

        #[async_trait]
        impl EthHttpRpcApi for EthHttpRpc {
            async fn block_number(&self) -> Result<U64>;
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

            async fn block(&self, block_number: U64) -> Result<Option<Block<H256>>>;
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

    fn key_change(block_number: u64, log_index: u8) -> EventWithCommon<KeyManagerEvent> {
        EventWithCommon::<KeyManagerEvent> {
            tx_hash: Default::default(),
            log_index: U256::from(log_index),
            block_number,
            event_parameters: KeyManagerEvent::KeyChange {
                signed: true,
                old_key: ChainflipKey::default(),
                new_key: ChainflipKey::default(),
            },
        }
    }

    fn block_events_with_event(
        block_number: u64,
        log_indices: Vec<u8>,
    ) -> Result<BlockEvents<KeyManagerEvent>> {
        Ok(BlockEvents {
            block_number,
            events: Some(
                log_indices
                    .into_iter()
                    .map(|index| key_change(block_number, index))
                    .collect(),
            ),
        })
    }

    fn block_events_no_events(block_number: u64) -> Result<BlockEvents<KeyManagerEvent>> {
        Ok(BlockEvents {
            block_number,
            events: None,
        })
    }

    // Generate a stream for each protocol, that, when selected upon, will return
    // in the order the items are passed in
    // This is useful to test more "real world" scenarios, as stream::iter will always
    // immediately yield, therefore items will always be pealed off the streams
    // alternatingly
    fn interleaved_streams(
        // contains the streams in the order they will return
        items: Vec<(Result<BlockEvents<KeyManagerEvent>>, TransportProtocol)>,
    ) -> (
        // ws
        impl Stream<Item = Result<BlockEvents<KeyManagerEvent>>>,
        // http
        impl Stream<Item = Result<BlockEvents<KeyManagerEvent>>>,
    ) {
        assert!(!items.is_empty(), "should have at least one item");

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
                total_delay_increment += DELAY_DURATION_MILLIS;
                total_delay_increment
            });

            match protocol {
                TransportProtocol::Http => http_items.push((item, delay)),
                TransportProtocol::Ws => ws_items.push((item, delay)),
            };

            protocol_last_returned = protocol;
        }

        let delayed_stream = |items: Vec<(Result<BlockEvents<KeyManagerEvent>>, Duration)>| {
            let items = items.into_iter();
            let item = Box::pin(stream::unfold(items, |mut items| async move {
                if let Some((i, d)) = items.next() {
                    tokio::time::sleep(d).await;
                    Some((i, items))
                } else {
                    None
                }
            }));
            item
        };

        (delayed_stream(ws_items), delayed_stream(http_items))
    }

    #[tokio::test]
    async fn empty_inners_returns_none() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let safe_ws_block_events_stream = Box::pin(stream::empty());
        let safe_http_block_events_stream = Box::pin(stream::empty());

        let mut stream = key_manager
            .merged_block_events_stream(
                safe_ws_block_events_stream,
                safe_http_block_events_stream,
                logger,
            )
            .await
            .unwrap();

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_does_not_return_duplicate_events() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let key_change_1 = 10;
        let key_change_2 = 13;
        let log_index = 0;
        let safe_ws_block_events_stream = Box::pin(stream::iter([
            block_events_with_event(key_change_1, vec![log_index]),
            // no events in these blocks
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(key_change_2, vec![log_index]),
        ]));
        let safe_http_block_events_stream = Box::pin(stream::iter([
            block_events_with_event(key_change_1, vec![log_index]),
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(key_change_2, vec![log_index]),
        ]));

        let mut stream = key_manager
            .merged_block_events_stream(
                safe_ws_block_events_stream,
                safe_http_block_events_stream,
                logger,
            )
            .await
            .unwrap();

        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_1, log_index)
        );
        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_2, log_index)
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_handles_broken_stream() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let safe_ws_block_events_stream = Box::pin(stream::empty());
        let safe_http_block_events_stream = Box::pin(stream::iter([
            block_events_with_event(10, vec![0]),
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(13, vec![0]),
        ]));

        let mut stream = key_manager
            .merged_block_events_stream(
                safe_ws_block_events_stream,
                safe_http_block_events_stream,
                logger,
            )
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(10, 0));
        assert_eq!(stream.next().await.unwrap(), key_change(13, 0));

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_handles_behind_stream() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let key_change_1 = 10;
        let key_change_2 = 13;
        let log_index = 0;

        // websockets is a block behind
        let safe_ws_block_events_stream = Box::pin(stream::iter([
            block_events_no_events(9),
            block_events_with_event(key_change_1, vec![log_index]),
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(13, vec![log_index]),
        ]));
        // http is a block ahead
        let safe_http_block_events_stream = Box::pin(stream::iter([
            block_events_with_event(key_change_1, vec![log_index]),
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(key_change_2, vec![log_index]),
            block_events_no_events(14),
        ]));

        let mut stream = key_manager
            .merged_block_events_stream(
                safe_ws_block_events_stream,
                safe_http_block_events_stream,
                logger,
            )
            .await
            .unwrap();

        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_1, log_index)
        );
        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_2, log_index)
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_handles_logs_in_same_tx() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let key_change_1 = 10;
        let key_change_2 = 13;
        let first_log_index = 0;
        let second_log_index = 2;
        let log_indices = vec![first_log_index, second_log_index];

        let safe_ws_block_events_stream = Box::pin(stream::iter([
            block_events_with_event(key_change_1, log_indices.clone()),
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(key_change_2, log_indices.clone()),
        ]));

        let safe_http_block_events_stream = Box::pin(stream::iter([
            block_events_with_event(key_change_1, log_indices.clone()),
            block_events_no_events(11),
            block_events_no_events(12),
            block_events_with_event(key_change_2, log_indices.clone()),
        ]));

        let mut stream = key_manager
            .merged_block_events_stream(
                safe_ws_block_events_stream,
                safe_http_block_events_stream,
                logger,
            )
            .await
            .unwrap();

        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_1, first_log_index)
        );
        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_1, second_log_index)
        );
        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_2, first_log_index)
        );
        assert_eq!(
            stream.next().await.unwrap(),
            key_change(key_change_2, second_log_index)
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn interleaved_streams_works_as_expected() {
        let items = vec![
            // yields nothing
            (block_events_no_events(10), TransportProtocol::Http),
            // returns
            (
                block_events_with_event(11, vec![0]),
                TransportProtocol::Http,
            ),
            // ignored
            (block_events_no_events(10), TransportProtocol::Ws),
            // ignored - already returned
            (block_events_with_event(11, vec![0]), TransportProtocol::Ws),
            // returns
            (block_events_with_event(12, vec![0]), TransportProtocol::Ws),
            // ignored
            (
                block_events_with_event(12, vec![0]),
                TransportProtocol::Http,
            ),
            // ignored / nothing interesting -  no return expected from these 4
            (block_events_no_events(13), TransportProtocol::Ws),
            (block_events_no_events(14), TransportProtocol::Ws),
            (block_events_no_events(13), TransportProtocol::Http),
            (block_events_no_events(14), TransportProtocol::Http),
            // returned
            (block_events_with_event(15, vec![0]), TransportProtocol::Ws),
            // ignored
            (
                block_events_with_event(15, vec![0]),
                TransportProtocol::Http,
            ),
        ];

        let (logger, mut tag_cache) = new_test_logger_with_tag_cache();
        let (ws_stream, http_stream) = interleaved_streams(items);

        let key_manager = test_km_contract();
        let mut stream = key_manager
            .merged_block_events_stream(ws_stream, http_stream, logger)
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(11, 0));
        assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_RETURNED));
        tag_cache.clear();

        assert_eq!(stream.next().await.unwrap(), key_change(12, 0));
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
        tag_cache.clear();

        assert_eq!(stream.next().await.unwrap(), key_change(15, 0));
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_RETURNED));
        tag_cache.clear();

        assert!(stream.next().await.is_none());
    }

    // merged_stream_notifies_once_every_x_blocks_when_one_falls_behind

    // merged_stream_continues_when_one_stream_moves_back_in_blocks

    // TODO: Test when the block events are skipped i.e. the block numbers are skipped. What should we do?

    // merged stream recovers when only one stream returns error block

    // merged stream terminates when both streams return error block

    // merged stream does not return blocks ahead of the failed block

    // merged stream ignores when the stream receives a failed block
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
