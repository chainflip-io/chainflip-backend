mod chain_data_witnessing;
mod http_safe_stream;
pub mod key_manager;
pub mod stake_manager;

pub mod event;

mod ws_safe_stream;

pub mod rpc;

pub mod utils;

use anyhow::{anyhow, bail, Context, Result};

use cf_traits::EpochIndex;
use pallet_cf_broadcast::BroadcastAttemptId;
use regex::Regex;
use sp_runtime::traits::Keccak256;
use state_chain_runtime::CfeSettings;
use utilities::make_periodic_tick;

use crate::{
    common::read_clean_and_decode_hex_str_file,
    constants::{
        ETH_BLOCK_SAFETY_MARGIN, ETH_FALLING_BEHIND_MARGIN_BLOCKS,
        ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL, ETH_STILL_BEHIND_LOG_INTERVAL,
    },
    eth::{
        http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL},
        rpc::{EthDualRpcClient, EthWsRpcApi},
        ws_safe_stream::safe_ws_head_stream,
    },
    logging::{COMPONENT_KEY, ETH_HTTP_STREAM_YIELDED, ETH_STREAM_BEHIND, ETH_WS_STREAM_YIELDED},
    settings,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
    task_scope::{with_task_scope, ScopedJoinHandle},
};
use chain_data_witnessing::*;
use ethbloom::{Bloom, Input};
use futures::{stream, FutureExt, StreamExt};
use slog::o;
use sp_core::{Hasher, H160, U256};
use std::{
    cmp::Ordering,
    fmt::{self, Debug},
    pin::Pin,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, watch};
use web3::{
    ethabi::{self, Address, Contract},
    signing::{Key, SecretKeyRef},
    types::{
        Block, BlockNumber, Bytes, CallRequest, FilterBuilder, TransactionParameters, H2048, H256,
        U64,
    },
};
use web3_secp256k1::SecretKey;

use tokio_stream::Stream;

use event::Event;

use async_trait::async_trait;

#[derive(Debug, PartialEq)]
pub struct EthNumberBloom {
    pub block_number: U64,
    pub logs_bloom: H2048,
    pub base_fee_per_gas: U256,
}

use self::rpc::{EthHttpRpcClient, EthRpcApi, EthWsRpcClient};

pub const ETH_CHAIN_TRACKING_POLL_INTERVAL: Duration = Duration::from_secs(4);

const EIP1559_TX_ID: u64 = 2;

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
    pub event: ethabi::Event,
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

/// Instructions for starting and ending witnessing. Start is inclusive, End is exclusive.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum ObserveInstruction {
    Start(
        <cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber,
        EpochIndex,
    ),
    End(<cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber),
}

/// Helper that generates a broadcast channel with multiple receivers.
pub fn build_broadcast_channel<T: Clone, const S: usize>(
    capacity: usize,
) -> (broadcast::Sender<T>, [broadcast::Receiver<T>; S]) {
    let (sender, _) = broadcast::channel(capacity);
    let receivers = [0; S].map(|_| sender.subscribe());
    (sender, receivers)
}

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start_contract_observer<ContractObserver, StateChainRpc>(
    contract_observer: ContractObserver,
    eth_ws_rpc: EthWsRpcClient,
    eth_http_rpc: EthHttpRpcClient,
    mut instruction_receiver: broadcast::Receiver<ObserveInstruction>,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    ContractObserver: 'static + EthObserver + Sync + Send,
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
{
    with_task_scope(|scope| {
        async {
            let logger = logger.new(
                o!(COMPONENT_KEY => format!("{}-Observer", contract_observer.contract_name())),
            );
            slog::info!(logger, "Starting");

            type TaskEndBlock = Arc<Mutex<Option<u64>>>;

            let mut option_handle_end_block: Option<(ScopedJoinHandle<()>, TaskEndBlock)> = None;

            let contract_observer = Arc::new(contract_observer);

            while let Ok(observe_instruction) = instruction_receiver.recv().await.map_err(|e| {
                slog::error!(logger, "Witnessing instruction receiver failed: {:?}", e)
            }) {
                match observe_instruction {
                    ObserveInstruction::End(to_block) => {
                        let (handle, end_at_block) = option_handle_end_block
                            .take()
                            .expect("Received two 'end' events in a row. This should not occur.");
                        // We already have a thread, we want to tell it when to stop and await on it
                        *end_at_block.lock().unwrap() = Some(to_block);
                        handle.await;
                    }
                    ObserveInstruction::Start(from_block, epoch) => {
                        assert!(
                            option_handle_end_block.is_none(),
                            "Received two 'start' events in a row. This should not occur."
                        );
                        option_handle_end_block = Some({
                            let task_end_at_block = Arc::new(Mutex::new(None));

                            // clone for capture by tokio task
                            let task_end_at_block_c = task_end_at_block.clone();
                            let eth_ws_rpc = eth_ws_rpc.clone();
                            let eth_http_rpc = eth_http_rpc.clone();
                            let dual_rpc = EthDualRpcClient::new(
                                eth_ws_rpc.clone(),
                                eth_http_rpc.clone(),
                                &logger,
                            );
                            let logger = logger.clone();
                            let contract_observer = contract_observer.clone();
                            let state_chain_client = state_chain_client.clone();
                            (
                                scope.spawn_with_handle(async move {
                                    slog::info!(
                                        logger,
                                        "Start observing from ETH block: {}",
                                        from_block
                                    );
                                    let mut block_stream = contract_observer
                                        .block_stream(eth_ws_rpc, eth_http_rpc, from_block, &logger)
                                        .await
                                        .expect("Failed to initialise block stream");

                                    // TOOD: Handle None on stream, and result event being an error
                                    while let Some(block) = block_stream.next().await {
                                        if let Some(end_at_block) =
                                            *task_end_at_block.lock().unwrap()
                                        {
                                            if block.block_number >= end_at_block {
                                                slog::info!(
                                                    logger,
                                                    "Finished observing events at ETH block: {}",
                                                    block.block_number
                                                );
                                                // we have reached the block height we wanted to witness up to
                                                // so can stop the witness process
                                                break;
                                            }
                                        }

                                        for event in block.events {
                                            contract_observer
                                                .handle_event(
                                                    epoch,
                                                    block.block_number,
                                                    event,
                                                    state_chain_client.clone(),
                                                    &dual_rpc,
                                                    &logger,
                                                )
                                                .await;
                                        }
                                    }

                                    Ok(())
                                }),
                                task_end_at_block_c,
                            )
                        })
                    }
                }
            }

            Ok(())
        }
        .boxed()
    })
    .await
}

pub async fn start_chain_data_witnesser<EthRpcClient, ScRpcClient>(
    eth_rpc: EthRpcClient,
    state_chain_client: Arc<StateChainClient<ScRpcClient>>,
    mut instruction_receiver: broadcast::Receiver<ObserveInstruction>,
    cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
    poll_interval: Duration,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    EthRpcClient: 'static + EthRpcApi + Clone + Send + Sync,
    ScRpcClient: 'static + StateChainRpcApi + Send + Sync,
{
    let logger = logger.new(o!(COMPONENT_KEY => "ETH-Chain-Data-Witnesser"));
    slog::info!(&logger, "Starting");

    with_task_scope(|scope| async {

        let (tracked_data_sender, mut tracked_data_receiver) = tokio::sync::mpsc::channel(10);

        scope.spawn({
            let logger = logger.clone();

            async move {
                while let Some(tracked_data) = tracked_data_receiver.recv().await {
                    state_chain_client
                        .submit_signed_extrinsic(
                            state_chain_runtime::Call::Witnesser(pallet_cf_witnesser::Call::witness {
                                call: Box::new(state_chain_runtime::Call::EthereumChainTracking(
                                    pallet_cf_chain_tracking::Call::update_chain_state {
                                        state: tracked_data,
                                    },
                                )),
                            }),
                            &logger,
                        )
                        .await
                        .context("Failed to submit signed extrinsic")?;
                }

                Ok(())
            }
        });

        let mut witnesser_task_handle: Option<(_, ScopedJoinHandle<_>)> = None;

        while let Ok(instruction) = instruction_receiver.recv().await {
            match instruction {
                ObserveInstruction::Start(start_block, _) => {
                    slog::info!(&logger, "Start observing from ETH block: {}", start_block);
                    assert!(
                        witnesser_task_handle.is_none(),
                        "Duplicate start event received."
                    );

                    let (to_block_sender, to_block_receiver) = oneshot::channel();

                    let _ref = witnesser_task_handle.insert((
                        to_block_sender,
                        scope.spawn_with_handle({
                            let eth_rpc = eth_rpc.clone();
                            let logger = logger.clone();
                            let tracked_data_sender = tracked_data_sender.clone();
                            let cfe_settings_update_receiver = cfe_settings_update_receiver.clone();

                            async move {
                                util::bounded(
                                    start_block,
                                    to_block_receiver,
                                    util::strictly_increasing(
                                        chain_data_witnessing::poll_latest_block_numbers(
                                            &eth_rpc,
                                            poll_interval,
                                            &logger,
                                        ),
                                    ),
                                )
                                .for_each(|block_number| {
                                    let eth_rpc_c = eth_rpc.clone();
                                    let logger = logger.clone();
                                    let tracked_data_sender = tracked_data_sender.clone();

                                    let priority_fee = cfe_settings_update_receiver
                                        .borrow()
                                        .eth_priority_fee_percentile;

                                    async move {
                                        match get_tracked_data(&eth_rpc_c, block_number, priority_fee).await {
                                            Ok(tracked_data) => {
                                                let _result = tracked_data_sender.send(tracked_data).await;
                                            }
                                            Err(e) => {
                                                slog::error!(
                                                    &logger,
                                                    "Failed to get tracked data: {:?}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                })
                                .await;

                                Ok(())
                            }
                        }),
                    ));
                }
                ObserveInstruction::End(end_block) => {
                    slog::info!(&logger, "End observing at ETH block: {}", end_block);

                    let (to_block_sender, join_handle) = witnesser_task_handle
                        .take()
                        .expect("Duplicate end event received, or end received before start.");
                    let _result = to_block_sender.send(end_block);
                    join_handle.await;
                }
            }
        }

        slog::info!(&logger, "Stopping witnesser");
        Ok(())
    }.boxed()).await
}

impl TryFrom<Block<H256>> for EthNumberBloom {
    type Error = anyhow::Error;

    fn try_from(block: Block<H256>) -> Result<Self, Self::Error> {
        if block.number.is_none() || block.logs_bloom.is_none() || block.base_fee_per_gas.is_none()
        {
            Err(anyhow!(
                "Block<H256> did not contain necessary block number and/or logs bloom and/or base fee per gas",
            ))
        } else {
            Ok(EthNumberBloom {
                block_number: block.number.unwrap(),
                logs_bloom: block.logs_bloom.unwrap(),
                base_fee_per_gas: block.base_fee_per_gas.unwrap(),
            })
        }
    }
}

/// Enables ETH event streaming via the `Web3` client and signing & broadcasting of txs
#[derive(Clone)]
pub struct EthBroadcaster<EthRpc>
where
    EthRpc: EthRpcApi,
{
    eth_rpc: EthRpc,
    secret_key: SecretKey,
    pub address: Address,
    logger: slog::Logger,
}

impl<EthRpc> EthBroadcaster<EthRpc>
where
    EthRpc: EthRpcApi,
{
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
        let tx_params = TransactionParameters {
            to: Some(unsigned_tx.contract),
            data: unsigned_tx.data.clone().into(),
            chain_id: Some(unsigned_tx.chain_id),
            value: unsigned_tx.value,
            max_fee_per_gas: unsigned_tx.max_fee_per_gas,
            max_priority_fee_per_gas: unsigned_tx.max_priority_fee_per_gas,
            transaction_type: Some(web3::types::U64::from(EIP1559_TX_ID)),
            gas: {
                let gas_estimate = match unsigned_tx.gas_limit {
                    None => {
                        // query for the gas estimate if the SC didn't provide it
                        let zero = Some(U256::from(0u64));
                        let call_request = CallRequest {
                            from: None,
                            to: unsigned_tx.contract.into(),
                            // Set the gas really high (~half gas in a block) for the estimate, since the estimation call requires you to
                            // input at least as much gas as the estimate will return
                            gas: Some(U256::from(15_000_000u64)),
                            gas_price: None,
                            value: unsigned_tx.value.into(),
                            data: Some(unsigned_tx.data.clone().into()),
                            transaction_type: Some(web3::types::U64::from(EIP1559_TX_ID)),
                            // Set the gas prices to zero for the estimate, so we don't get
                            // rejected for not having enough ETH
                            max_fee_per_gas: zero,
                            max_priority_fee_per_gas: zero,
                            ..Default::default()
                        };

                        self.eth_rpc
                            .estimate_gas(call_request, None)
                            .await
                            .context("Failed to estimate gas")?
                    }
                    Some(gas_limit) => gas_limit,
                };
                // increase the estimate by 50%
                let gas = gas_estimate
                    .saturating_mul(U256::from(3u64))
                    .checked_div(U256::from(2u64))
                    .unwrap();

                slog::debug!(
                    self.logger,
                    "Gas estimate for unsigned tx: {:?} is {}. Setting 50% higher at: {}",
                    unsigned_tx,
                    gas_estimate,
                    gas
                );

                gas
            },
            ..Default::default()
        };

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
            .context("Failed to broadcast ETH transaction to network")
    }

    /// Does a `send` but with extra logging and error handling, related to a broadcast
    pub async fn send_for_broadcast_attempt(
        &self,
        raw_signed_tx: Vec<u8>,
        broadcast_attempt_id: BroadcastAttemptId,
    ) {
        let expected_broadcast_tx_hash = Keccak256::hash(&raw_signed_tx[..]);
        match self.send(raw_signed_tx).await {
            Ok(tx_hash) => {
                slog::debug!(
                    self.logger,
                    "Successful TransmissionRequest broadcast_attempt_id {}, tx_hash: {:#x}",
                    broadcast_attempt_id,
                    tx_hash
                );
                assert_eq!(
                    tx_hash, expected_broadcast_tx_hash,
                    "tx_hash returned from `send` does not match expected hash"
                );
            }
            Err(e) => {
                slog::info!(
                    self.logger,
                    "TransmissionRequest broadcast_attempt_id {} failed: {:?}",
                    broadcast_attempt_id,
                    e
                );
            }
        }
    }
}

// Used to zip on the streams, so we know which stream is returning
#[derive(Clone, PartialEq, Debug, Copy)]
pub enum TransportProtocol {
    Http,
    Ws,
}

impl fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TransportProtocol::Ws => write!(f, "WebSocket"),
            TransportProtocol::Http => write!(f, "HTTP"),
        }
    }
}

/// Contains empty vec when no interesting events
/// Ok if the logs decode successfully, error if not
#[derive(Debug)]
pub struct BlockWithDecodedEvents<EventParameters: Debug> {
    pub block_number: u64,
    pub decode_events_result: Result<Vec<Event<EventParameters>>>,
}

/// Just contains an empty vec if there are no events
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct BlockWithEvents<EventParameters: Debug> {
    pub block_number: u64,
    pub events: Vec<Event<EventParameters>>,
}

#[async_trait]
pub trait EthObserver {
    type EventParameters: Debug + Send + Sync + 'static;

    fn contract_name(&self) -> &'static str;

    /// Takes a head stream and turns it into a stream of BlockEvents for consumption by the merged stream
    async fn block_events_stream_from_head_stream<BlockHeaderStream, EthRpc>(
        &self,
        from_block: u64,
        contract_address: H160,
        safe_head_stream: BlockHeaderStream,
        eth_rpc: EthRpc,
        logger: slog::Logger,
    ) -> Result<
        Pin<Box<dyn Stream<Item = BlockWithDecodedEvents<Self::EventParameters>> + Send + '_>>,
    >
    where
        BlockHeaderStream: Stream<Item = EthNumberBloom> + 'static + Send,
        EthRpc: 'static + EthRpcApi + Send + Sync + Clone,
    {
        let from_block = U64::from(from_block);
        let mut safe_head_stream = Box::pin(safe_head_stream);

        // only allow pulling from the stream once we are actually at our from_block number
        while let Some(best_safe_block_header) = safe_head_stream.next().await {
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

                let eth_rpc_c = eth_rpc.clone();

                let past_heads = Box::pin(
                    stream::iter(from_block.as_u64()..=best_safe_block_number.as_u64()).then(
                        move |block_number| {
                            let eth_rpc = eth_rpc_c.clone();
                            async move {
                                eth_rpc
                                    .block(U64::from(block_number))
                                    .await
                                    .and_then(|block| {
                                        let number_bloom: Result<EthNumberBloom> = block.try_into();
                                        number_bloom
                                    })
                            }
                        },
                    ),
                );

                let past_and_fut_heads = stream::unfold(
                    (past_heads, safe_head_stream),
                    |(mut past_heads, mut safe_head_stream)| async {
                        // we want to consume the past logs stream first, terminating if any of these logs are an error
                        if let Some(result_past_log) = past_heads.next().await {
                            if let Ok(past_log) = result_past_log {
                                Some((past_log, (past_heads, safe_head_stream)))
                            } else {
                                None
                            }
                        } else {
                            // the past logs were consumed, now we consume the "future" logs
                            safe_head_stream
                                .next()
                                .await
                                .map(|future_log| (future_log, (past_heads, safe_head_stream)))
                        }
                    },
                )
                .fuse();
                let eth_rpc_c = eth_rpc.clone();

                let decode_log_fn = self.decode_log_closure()?;

                // convert from heads to events
                let events = past_and_fut_heads
                    .then(move |header| {
                        let eth_rpc = eth_rpc_c.clone();

                        async move {
                            let block_number = header.block_number;
                            let mut contract_bloom = Bloom::default();
                            contract_bloom.accrue(Input::Raw(&contract_address.0));

                            // if we have logs for this block, fetch them.
                            let result_logs = if header.logs_bloom.contains_bloom(&contract_bloom) {
                                eth_rpc
                                    .get_logs(
                                        FilterBuilder::default()
                                            // from_block *and* to_block are *inclusive*
                                            .from_block(BlockNumber::Number(block_number))
                                            .to_block(BlockNumber::Number(block_number))
                                            .address(vec![contract_address])
                                            .build(),
                                    )
                                    .await
                            } else {
                                // we know there won't be interesting logs, so don't fetch for events
                                Ok(vec![])
                            };

                            (block_number.as_u64(), result_logs)
                        }
                    })
                    .map(move |(block_number, result_logs)| BlockWithDecodedEvents {
                        block_number,
                        decode_events_result: result_logs.and_then(|logs| {
                            logs.into_iter()
                                    .map(
                                        |unparsed_log| -> Result<
                                            Event<Self::EventParameters>,
                                            anyhow::Error,
                                        > {
                                            Event::<Self::EventParameters>::new_from_unparsed_logs(
                                                &decode_log_fn,
                                                unparsed_log,
                                            )
                                        },
                                    )
                                    .collect::<Result<Vec<_>>>()
                        }),
                    });

                return Ok(Box::pin(events));
            }
        }
        Err(anyhow!("No events in ETH safe head stream"))
    }

    /// Get an block stream for the contract, returning the stream only if the head of the stream is
    /// ahead of from_block (otherwise it will wait until we have reached from_block)
    async fn block_stream(
        &self,
        eth_ws_rpc: EthWsRpcClient,
        eth_http_rpc: EthHttpRpcClient,
        // usually the start of the validator's active window
        from_block: u64,
        logger: &slog::Logger,
        // This stream must be Send, so it can be used by the spawn
    ) -> Result<Pin<Box<dyn Stream<Item = BlockWithEvents<Self::EventParameters>> + Send + '_>>>
    {
        let deployed_address = self.get_contract_address();
        slog::info!(
            logger,
            "Subscribing to ETH events from contract at address: {:?}",
            hex::encode(deployed_address)
        );

        let eth_head_stream = eth_ws_rpc.subscribe_new_heads().await?;

        let safe_ws_head_stream =
            safe_ws_head_stream(eth_head_stream, ETH_BLOCK_SAFETY_MARGIN, logger);

        let safe_ws_block_events = self
            .block_events_stream_from_head_stream(
                from_block,
                deployed_address,
                safe_ws_head_stream,
                eth_ws_rpc,
                logger.clone(),
            )
            .await?;

        let safe_http_head_stream = safe_polling_http_head_stream(
            eth_http_rpc.clone(),
            HTTP_POLL_INTERVAL,
            ETH_BLOCK_SAFETY_MARGIN,
            logger,
        )
        .await;

        let safe_http_block_events = self
            .block_events_stream_from_head_stream(
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
        &self,
        safe_ws_block_events_stream: BlockEventsStreamWs,
        safe_http_block_events_stream: BlockEventsStreamHttp,
        logger: slog::Logger,
    ) -> Result<Pin<Box<dyn Stream<Item = BlockWithEvents<Self::EventParameters>> + Send + 'a>>>
    where
        BlockEventsStreamWs:
            Stream<Item = BlockWithDecodedEvents<Self::EventParameters>> + Unpin + Send + 'a,
        BlockEventsStreamHttp:
            Stream<Item = BlockWithDecodedEvents<Self::EventParameters>> + Unpin + Send + 'a,
    {
        #[derive(Debug)]
        struct ProtocolState {
            last_block_pulled: u64,
            log_ticker: tokio::time::Interval,
            protocol: TransportProtocol,
        }
        #[derive(Debug)]
        struct MergedStreamState {
            last_block_yielded: u64,
            logger: slog::Logger,
        }

        struct StreamState<BlockEventsStreamWs: Stream, BlockEventsStreamHttp: Stream> {
            ws_state: ProtocolState,
            ws_stream: BlockEventsStreamWs,
            http_state: ProtocolState,
            http_stream: BlockEventsStreamHttp,
            merged_stream_state: MergedStreamState,
        }

        let init_state = StreamState::<BlockEventsStreamWs, BlockEventsStreamHttp> {
            ws_state: ProtocolState {
                last_block_pulled: 0,
                log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL, false),
                protocol: TransportProtocol::Ws,
            },
            ws_stream: safe_ws_block_events_stream,
            http_state: ProtocolState {
                last_block_pulled: 0,
                log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL, false),
                protocol: TransportProtocol::Http,
            },
            http_stream: safe_http_block_events_stream,
            merged_stream_state: MergedStreamState {
                last_block_yielded: 0,
                logger,
            },
        };

        fn log_when_yielding(
            yielding_stream_state: &ProtocolState,
            non_yielding_stream_state: &ProtocolState,
            merged_stream_state: &MergedStreamState,
            yielding_block_number: u64,
        ) {
            match yielding_stream_state.protocol {
                TransportProtocol::Http => {
                    slog::info!(
                        merged_stream_state.logger,
                        #ETH_HTTP_STREAM_YIELDED,
                        "ETH block {} returning from {} stream",
                        yielding_block_number,
                        yielding_stream_state.protocol
                    );
                }
                TransportProtocol::Ws => {
                    slog::info!(
                        merged_stream_state.logger,
                        #ETH_WS_STREAM_YIELDED,
                        "ETH block {} returning from {} stream",
                        yielding_block_number,
                        yielding_stream_state.protocol
                    );
                }
            }

            // We may be one ahead of the previously yielded block
            let blocks_behind = merged_stream_state.last_block_yielded + 1
                - non_yielding_stream_state.last_block_pulled;

            // before we have pulled on each stream, we can't know if the other stream is behind
            if non_yielding_stream_state.last_block_pulled != 0
                && ((non_yielding_stream_state.last_block_pulled
                    + ETH_FALLING_BEHIND_MARGIN_BLOCKS)
                    <= yielding_block_number)
                && (blocks_behind % ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL == 0)
            {
                slog::warn!(
                    merged_stream_state.logger,
                    #ETH_STREAM_BEHIND,
                    "ETH {} stream at block `{}` but {} stream at block `{}`",
                    yielding_stream_state.protocol,
                    yielding_block_number,
                    non_yielding_stream_state.protocol,
                    non_yielding_stream_state.last_block_pulled,
                );
            }
        }

        // Returns Error if:
        // 1. the protocol stream does not return a contiguous sequence of blocks
        // 2. the protocol streams have not started at the same block
        // 3. failure in `recover_with_other_stream`
        // When returning Ok, will return None if:
        // 1. the protocol stream is behind the next block to yield
        async fn do_for_protocol<
            BlockEventsStream: Stream<Item = BlockWithDecodedEvents<EventParameters>> + Unpin,
            EventParameters: Debug,
        >(
            merged_stream_state: &mut MergedStreamState,
            protocol_state: &mut ProtocolState,
            other_protocol_state: &mut ProtocolState,
            mut other_protocol_stream: BlockEventsStream,
            block_events: BlockWithDecodedEvents<EventParameters>,
        ) -> Result<Option<BlockWithEvents<EventParameters>>> {
            let next_block_to_yield = merged_stream_state.last_block_yielded + 1;
            let merged_has_yielded = merged_stream_state.last_block_yielded != 0;
            let has_pulled = protocol_state.last_block_pulled != 0;

            assert!(!has_pulled
                || (block_events.block_number == protocol_state.last_block_pulled + 1), "ETH {} stream is expected to be a contiguous sequence of block events. Last pulled `{}`, got `{}`", protocol_state.protocol, protocol_state.last_block_pulled, block_events.block_number);

            protocol_state.last_block_pulled = block_events.block_number;

            let opt_block_events = if merged_has_yielded {
                if block_events.block_number == next_block_to_yield {
                    Some(block_events)
                    // if we're only one block "behind" we're not really "behind", we were just the second stream polled
                } else if block_events.block_number + 1 < next_block_to_yield {
                    None
                } else if block_events.block_number < next_block_to_yield {
                    // we're behind, but we only want to log once every interval
                    if protocol_state.log_ticker.tick().now_or_never().is_some() {
                        slog::trace!(merged_stream_state.logger, "ETH {} stream pulled block {}. But this is behind the next block to yield of {}. Continuing...", protocol_state.protocol, block_events.block_number, next_block_to_yield);
                    }
                    None
                } else {
                    panic!("Input streams to merged stream started at different block numbers. This should not occur.");
                }
            } else {
                // yield
                Some(block_events)
            };

            if let Some(block_events) = opt_block_events {
                match block_events.decode_events_result {
                    Ok(events) => {
                        // yield, if we are at high enough block number
                        log_when_yielding(
                            protocol_state,
                            other_protocol_state,
                            merged_stream_state,
                            block_events.block_number,
                        );
                        Ok(Some(BlockWithEvents {
                            block_number: block_events.block_number,
                            events,
                        }))
                    }
                    Err(err) => {
                        slog::error!(
                            merged_stream_state.logger,
                            "ETH {} stream failed to get events for ETH block `{}`. Attempting to recover. Error: {}",
                            protocol_state.protocol,
                            block_events.block_number,
                            err
                        );
                        while let Some(block_events) = other_protocol_stream.next().await {
                            other_protocol_state.last_block_pulled = block_events.block_number;
                            match block_events.block_number.cmp(&next_block_to_yield) {
                                Ordering::Equal => {
                                    // we want to yield this one :)
                                    match block_events.decode_events_result {
                                        Ok(events) => {
                                            log_when_yielding(
                                                other_protocol_state,
                                                protocol_state,
                                                merged_stream_state,
                                                block_events.block_number,
                                            );
                                            return Ok(Some(BlockWithEvents {
                                                block_number: block_events.block_number,
                                                events,
                                            }));
                                        }
                                        Err(err) => {
                                            bail!("ETH {} stream failed with error, on block {} that we were recovering from: {}", other_protocol_state.protocol, block_events.block_number, err);
                                        }
                                    }
                                }
                                Ordering::Less => {
                                    slog::trace!(merged_stream_state.logger, "ETH {} stream pulled block `{}` but still below the next block to yield of {}", other_protocol_state.protocol, block_events.block_number, next_block_to_yield)
                                }
                                Ordering::Greater => {
                                    // This is ensured by the safe streams
                                    panic!(
                                        "ETH {} stream skipped blocks. Next block to yield was `{}` but got block `{}`. This should not occur",
                                        other_protocol_state.protocol,
                                        next_block_to_yield,
                                        block_events.block_number
                                    );
                                }
                            }
                        }

                        bail!(
                            "ETH {} stream terminated when attempting to recover",
                            other_protocol_state.protocol,
                        );
                    }
                }
            } else {
                Ok(None)
            }
        }

        Ok(Box::pin(stream::unfold(
            init_state,
            |mut stream_state| async move {
                loop {
                    let next_clean_block_events = tokio::select! {
                        Some(block_events) = stream_state.ws_stream.next() => {
                            do_for_protocol(&mut stream_state.merged_stream_state, &mut stream_state.ws_state, &mut stream_state.http_state, &mut stream_state.http_stream, block_events).await
                        }
                        Some(block_events) = stream_state.http_stream.next() => {
                            do_for_protocol(&mut stream_state.merged_stream_state, &mut stream_state.http_state, &mut stream_state.ws_state, &mut stream_state.ws_stream, block_events).await
                        }
                        else => break None
                    };

                    match next_clean_block_events {
                        Ok(opt_clean_block_events) => {
                            if let Some(clean_block_events) = opt_clean_block_events {
                                stream_state.merged_stream_state.last_block_yielded =
                                    clean_block_events.block_number;
                                break Some((clean_block_events, stream_state));
                            }
                        }
                        Err(err) => {
                            slog::error!(
                                stream_state.merged_stream_state.logger,
                                "Terminating ETH merged block stream due to error: {}",
                                err
                            );
                            break None;
                        }
                    }
                }
            },
        )))
    }

    fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>>;

    async fn handle_event<RpcClient, EthRpcClient>(
        &self,
        epoch: EpochIndex,
        block_number: u64,
        event: Event<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        eth_rpc: &EthRpcClient,
        logger: &slog::Logger,
    ) where
        RpcClient: 'static + StateChainRpcApi + Sync + Send,
        EthRpcClient: EthRpcApi + Sync + Send;

    fn get_contract_address(&self) -> H160;
}

pub type DecodeLogClosure<EventParameters> =
    Box<dyn Fn(H256, ethabi::RawLog) -> Result<EventParameters> + Send + Sync>;

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
mod merged_stream_tests {
    use std::time::Duration;

    use utilities::assert_future_panics;

    use crate::logging::test_utils::new_test_logger;
    use crate::logging::test_utils::new_test_logger_with_tag_cache;
    use crate::logging::ETH_WS_STREAM_YIELDED;

    use super::key_manager::ChainflipKey;
    use super::key_manager::KeyManagerEvent;

    use super::key_manager::KeyManager;

    use super::*;

    // Arbitrariily chosen one of the EthRpc's for these tests
    fn test_km_contract() -> KeyManager {
        KeyManager::new(H160::default())
    }

    fn make_dummy_events(log_indices: &[u8]) -> Vec<Event<KeyManagerEvent>> {
        log_indices
            .iter()
            .map(|log_index| Event::<KeyManagerEvent> {
                tx_hash: Default::default(),
                log_index: U256::from(*log_index),
                event_parameters: KeyManagerEvent::AggKeySetByAggKey {
                    old_agg_key: ChainflipKey::default(),
                    new_agg_key: ChainflipKey::default(),
                },
            })
            .collect()
    }

    fn block_with_ok_events_decoding(
        block_number: u64,
        log_indices: &[u8],
    ) -> BlockWithDecodedEvents<KeyManagerEvent> {
        BlockWithDecodedEvents {
            block_number,
            decode_events_result: Ok(make_dummy_events(log_indices)),
        }
    }

    fn block_with_err_events_decoding(
        block_number: u64,
    ) -> BlockWithDecodedEvents<KeyManagerEvent> {
        BlockWithDecodedEvents {
            block_number,
            decode_events_result: Err(anyhow!("NOOOO")),
        }
    }

    fn block_with_events(
        block_number: u64,
        log_indices: &[u8],
    ) -> BlockWithEvents<KeyManagerEvent> {
        BlockWithEvents {
            block_number,
            events: make_dummy_events(log_indices),
        }
    }

    async fn test_merged_stream_interleaving(
        interleaved_blocks: Vec<(BlockWithDecodedEvents<KeyManagerEvent>, TransportProtocol)>,
        expected_blocks: &[(BlockWithEvents<KeyManagerEvent>, TransportProtocol)],
    ) {
        // Generate a stream for each protocol, that, when selected upon, will return
        // in the order the blocks are passed in
        // This is useful to test more "real world" scenarios, as stream::iter will always
        // immediately yield, therefore blocks will always be pealed off the streams
        // alternatingly
        let (ws_stream, http_stream) = {
            assert!(
                !interleaved_blocks.is_empty(),
                "should have at least one item"
            );

            const DELAY_DURATION_MILLIS: u64 = 50;

            let mut protocol_last_returned = interleaved_blocks.first().unwrap().1;
            let mut http_blocks = Vec::new();
            let mut ws_blocks = Vec::new();
            let mut total_delay_increment = 0;

            for (block, protocol) in interleaved_blocks {
                // if we are returning the same, we can just go the next, since we are ordered
                let delay = Duration::from_millis(if protocol == protocol_last_returned {
                    0
                } else {
                    total_delay_increment += DELAY_DURATION_MILLIS;
                    total_delay_increment
                });

                match protocol {
                    TransportProtocol::Http => http_blocks.push((block, delay)),
                    TransportProtocol::Ws => ws_blocks.push((block, delay)),
                };

                protocol_last_returned = protocol;
            }

            let delayed_stream =
                |blocks: Vec<(BlockWithDecodedEvents<KeyManagerEvent>, Duration)>| {
                    let blocks = blocks.into_iter();
                    Box::pin(
                        stream::unfold(blocks, |mut blocks| async move {
                            if let Some((i, d)) = blocks.next() {
                                tokio::time::sleep(d).await;
                                Some((i, blocks))
                            } else {
                                None
                            }
                        })
                        .fuse(),
                    )
                };

            (delayed_stream(ws_blocks), delayed_stream(http_blocks))
        };

        let (logger, mut tag_cache) = new_test_logger_with_tag_cache();

        assert_eq!(
            test_km_contract()
                .merged_block_events_stream(ws_stream, http_stream, logger)
                .await
                .unwrap()
                .map(move |x| {
                    (x, {
                        let protocol = if tag_cache.contains_tag(ETH_WS_STREAM_YIELDED)
                            && !tag_cache.contains_tag(ETH_HTTP_STREAM_YIELDED)
                        {
                            TransportProtocol::Ws
                        } else if !tag_cache.contains_tag(ETH_WS_STREAM_YIELDED)
                            && tag_cache.contains_tag(ETH_HTTP_STREAM_YIELDED)
                        {
                            TransportProtocol::Http
                        } else {
                            panic!()
                        };
                        tag_cache.clear();
                        protocol
                    })
                })
                .collect::<Vec<_>>()
                .await,
            expected_blocks
        );
    }

    #[tokio::test]
    async fn empty_inners_returns_none() {
        assert!(test_km_contract()
            .merged_block_events_stream(
                Box::pin(stream::empty()),
                Box::pin(stream::empty()),
                new_test_logger(),
            )
            .await
            .unwrap()
            .next()
            .await
            .is_none());
    }

    #[tokio::test]
    async fn merged_does_not_return_duplicate_blocks() {
        assert_eq!(
            test_km_contract()
                .merged_block_events_stream(
                    Box::pin(stream::iter([
                        block_with_ok_events_decoding(10, &[0]),
                        block_with_ok_events_decoding(11, &[]),
                        block_with_ok_events_decoding(12, &[]),
                        block_with_ok_events_decoding(13, &[0]),
                    ])),
                    Box::pin(stream::iter([
                        block_with_ok_events_decoding(10, &[0]),
                        block_with_ok_events_decoding(11, &[]),
                        block_with_ok_events_decoding(12, &[]),
                        block_with_ok_events_decoding(13, &[0]),
                    ])),
                    new_test_logger(),
                )
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await,
            &[
                block_with_events(10, &[0]),
                block_with_events(11, &[]),
                block_with_events(12, &[]),
                block_with_events(13, &[0]),
            ]
        );
    }

    #[tokio::test]
    async fn merged_stream_handles_broken_stream() {
        assert_eq!(
            test_km_contract()
                .merged_block_events_stream(
                    Box::pin(stream::empty()),
                    Box::pin(stream::iter([
                        block_with_ok_events_decoding(10, &[0]),
                        block_with_ok_events_decoding(11, &[]),
                        block_with_ok_events_decoding(12, &[]),
                        block_with_ok_events_decoding(13, &[0]),
                    ])),
                    new_test_logger(),
                )
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await,
            &[
                block_with_events(10, &[0]),
                block_with_events(11, &[]),
                block_with_events(12, &[]),
                block_with_events(13, &[0]),
            ]
        );
    }

    #[tokio::test]
    async fn interleaved_streams_works_as_expected() {
        test_merged_stream_interleaving(
            vec![
                (
                    block_with_ok_events_decoding(10, &[]),
                    TransportProtocol::Http,
                ), // returned
                (
                    block_with_ok_events_decoding(11, &[0]),
                    TransportProtocol::Http,
                ), // returned
                (
                    block_with_ok_events_decoding(10, &[]),
                    TransportProtocol::Ws,
                ), // ignored
                (
                    block_with_ok_events_decoding(11, &[0]),
                    TransportProtocol::Ws,
                ), // ignored
                (
                    block_with_ok_events_decoding(12, &[0]),
                    TransportProtocol::Ws,
                ), // returned
                (
                    block_with_ok_events_decoding(12, &[0]),
                    TransportProtocol::Http,
                ), // ignored
                (
                    block_with_ok_events_decoding(13, &[]),
                    TransportProtocol::Ws,
                ), // returned
                (
                    block_with_ok_events_decoding(14, &[]),
                    TransportProtocol::Ws,
                ), // returned
                (
                    block_with_ok_events_decoding(13, &[]),
                    TransportProtocol::Http,
                ), // ignored
                (
                    block_with_ok_events_decoding(14, &[]),
                    TransportProtocol::Http,
                ), // ignored
                (
                    block_with_ok_events_decoding(15, &[0]),
                    TransportProtocol::Ws,
                ), // returned
                (
                    block_with_ok_events_decoding(15, &[0]),
                    TransportProtocol::Http,
                ), // ignored
            ],
            &[
                (block_with_events(10, &[]), TransportProtocol::Http),
                (block_with_events(11, &[0]), TransportProtocol::Http),
                (block_with_events(12, &[0]), TransportProtocol::Ws),
                (block_with_events(13, &[]), TransportProtocol::Ws),
                (block_with_events(14, &[]), TransportProtocol::Ws),
                (block_with_events(15, &[0]), TransportProtocol::Ws),
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn merged_stream_notifies_once_every_x_blocks_when_one_falls_behind() {
        let (logger, tag_cache) = new_test_logger_with_tag_cache();

        let ws_range = 10..54;

        assert!(Iterator::eq(
            test_km_contract()
                .merged_block_events_stream(
                    stream::iter(
                        ws_range
                            .clone()
                            .map(|n| block_with_ok_events_decoding(n, &[0]))
                    ),
                    stream::iter([block_with_ok_events_decoding(10, &[0])]),
                    logger
                )
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await
                .into_iter(),
            ws_range.map(|i| block_with_events(i, &[0]))
        ));
        assert_eq!(tag_cache.get_tag_count(ETH_STREAM_BEHIND), 4);
    }

    #[tokio::test]
    async fn merged_stream_panics_if_a_stream_moves_backwards() {
        let mut stream = test_km_contract()
            .merged_block_events_stream(
                Box::pin(stream::iter([
                    block_with_ok_events_decoding(12, &[0]),
                    block_with_ok_events_decoding(13, &[]),
                    block_with_ok_events_decoding(14, &[2]),
                    // We jump back here
                    block_with_ok_events_decoding(13, &[]),
                    block_with_ok_events_decoding(15, &[]),
                    block_with_ok_events_decoding(16, &[0]),
                ])),
                Box::pin(stream::iter([
                    block_with_ok_events_decoding(12, &[0]),
                    block_with_ok_events_decoding(13, &[]),
                    block_with_ok_events_decoding(14, &[2]),
                    // We jump back here
                    block_with_ok_events_decoding(13, &[]),
                    block_with_ok_events_decoding(15, &[]),
                    block_with_ok_events_decoding(16, &[0]),
                ])),
                new_test_logger(),
            )
            .await
            .unwrap();

        stream.next().await.unwrap();
        stream.next().await.unwrap();
        stream.next().await.unwrap();
        assert_future_panics!(stream.next());
    }

    #[tokio::test]
    async fn merged_stream_recovers_when_one_stream_errors_and_other_catches_up_with_success() {
        test_merged_stream_interleaving(
            vec![
                (
                    block_with_ok_events_decoding(5, &[]),
                    TransportProtocol::Http,
                ),
                (
                    block_with_ok_events_decoding(6, &[0]),
                    TransportProtocol::Http,
                ),
                (
                    block_with_ok_events_decoding(7, &[]),
                    TransportProtocol::Http,
                ),
                (
                    block_with_ok_events_decoding(8, &[]),
                    TransportProtocol::Http,
                ),
                (
                    block_with_ok_events_decoding(9, &[]),
                    TransportProtocol::Http,
                ),
                // we had some events, but they are an error
                (block_with_err_events_decoding(10), TransportProtocol::Http),
                // so now we should enter recovery on the websockets stream
                (block_with_ok_events_decoding(5, &[]), TransportProtocol::Ws),
                (
                    block_with_ok_events_decoding(6, &[0]),
                    TransportProtocol::Ws,
                ),
                (block_with_ok_events_decoding(7, &[]), TransportProtocol::Ws),
                (block_with_ok_events_decoding(8, &[]), TransportProtocol::Ws),
                (block_with_ok_events_decoding(9, &[]), TransportProtocol::Ws),
                (
                    block_with_ok_events_decoding(10, &[4]),
                    TransportProtocol::Ws,
                ),
            ],
            &[
                (block_with_events(5, &[]), TransportProtocol::Http),
                (block_with_events(6, &[0]), TransportProtocol::Http),
                (block_with_events(7, &[]), TransportProtocol::Http),
                (block_with_events(8, &[]), TransportProtocol::Http),
                (block_with_events(9, &[]), TransportProtocol::Http),
                (block_with_events(10, &[4]), TransportProtocol::Ws),
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn merged_stream_exits_when_both_streams_have_error_events_for_a_block() {
        assert_eq!(
            test_km_contract()
                .merged_block_events_stream(
                    Box::pin(stream::iter([
                        block_with_ok_events_decoding(11, &[0]),
                        block_with_err_events_decoding(12),
                    ])),
                    Box::pin(stream::iter([
                        block_with_ok_events_decoding(11, &[0]),
                        block_with_err_events_decoding(12),
                    ])),
                    new_test_logger()
                )
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await,
            &[block_with_events(11, &[0])]
        );
    }
}

#[cfg(test)]
mod witnesser_tests {
    use web3::types::TransactionReceipt;

    use super::rpc::mocks::MockEthHttpRpcClient;
    use super::*;
    use crate::logging::test_utils::new_test_logger;
    use crate::state_chain_observer::client::MockStateChainRpcApi;
    use crate::task_scope::with_main_task_scope;

    #[test]
    fn test_start_chain_data_witnesser() {
        struct ClonableRpc<T>(Arc<T>);

        impl<T> Clone for ClonableRpc<T> {
            fn clone(&self) -> Self {
                ClonableRpc(self.0.clone())
            }
        }

        impl<T> ClonableRpc<T> {
            pub fn new(rpc: T) -> Self {
                Self(Arc::new(rpc))
            }
        }

        #[async_trait]
        impl<T: EthRpcApi> EthRpcApi for ClonableRpc<T> {
            async fn estimate_gas(
                &self,
                req: CallRequest,
                block: Option<BlockNumber>,
            ) -> Result<U256> {
                self.0.estimate_gas(req, block).await
            }

            async fn sign_transaction(
                &self,
                tx: TransactionParameters,
                key: &SecretKey,
            ) -> Result<web3::types::SignedTransaction> {
                self.0.sign_transaction(tx, key).await
            }

            async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
                self.0.send_raw_transaction(rlp).await
            }

            async fn get_logs(&self, filter: web3::types::Filter) -> Result<Vec<web3::types::Log>> {
                self.0.get_logs(filter).await
            }

            async fn chain_id(&self) -> Result<U256> {
                self.0.chain_id().await
            }

            async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt> {
                self.0.transaction_receipt(tx_hash).await
            }

            async fn block(&self, block_number: U64) -> Result<Block<H256>> {
                self.0.block(block_number).await
            }

            async fn fee_history(
                &self,
                block_count: U256,
                newest_block: BlockNumber,
                reward_percentiles: Option<Vec<f64>>,
            ) -> Result<web3::types::FeeHistory> {
                self.0
                    .fee_history(block_count, newest_block, reward_percentiles)
                    .await
            }

            /// Get the latest block number.
            async fn block_number(&self) -> Result<U64> {
                self.0.block_number().await
            }
        }

        let logger = new_test_logger();

        let mut eth_rpc = MockEthHttpRpcClient::new();
        let mut sc_rpc = MockStateChainRpcApi::new();

        // ** Rpc Api Assumptions **
        const BASE_FEE: u128 = 40;
        const PRIORITY_FEE: u128 = 5;
        const START: usize = 2;
        const END: usize = 10;
        const REPEATS: usize = 4;

        let mut block_numbers = (0u64..).map(web3::types::U64::from);
        eth_rpc.expect_block_number().returning(move || {
            block_numbers
                .next()
                .ok_or_else(|| anyhow::anyhow!("No more block numbers"))
        });

        eth_rpc
            .expect_fee_history()
            .returning(|_, block_number, _| {
                Ok(web3::types::FeeHistory {
                    oldest_block: block_number,
                    base_fee_per_gas: vec![U256::from(BASE_FEE)],
                    gas_used_ratio: vec![],
                    reward: Some(vec![vec![U256::from(PRIORITY_FEE)]]),
                })
            });

        sc_rpc
            .expect_submit_extrinsic_rpc()
            .times(REPEATS * (START..END).count())
            .returning(|_| Ok(Default::default()));
        // ** Rpc Api Assumptions **

        let eth_rpc = ClonableRpc::new(eth_rpc);
        with_main_task_scope(|scope| {
            async {
                let sc_client = Arc::new(StateChainClient::create_test_sc_client(sc_rpc));
                let (instruction_sender, [instruction_receiver]) = build_broadcast_channel(10);
                let (_, cfe_settings_update_receiver) =
                    watch::channel::<CfeSettings>(CfeSettings::default());
                scope.spawn(start_chain_data_witnesser(
                    eth_rpc.clone(),
                    sc_client,
                    instruction_receiver,
                    cfe_settings_update_receiver,
                    Duration::from_millis(10),
                    &logger,
                ));

                for i in 0..REPEATS {
                    let offset = i * END;
                    instruction_sender
                        .send(ObserveInstruction::Start((offset + START) as u64, i as u32))
                        .unwrap();
                    instruction_sender
                        .send(ObserveInstruction::End((offset + END) as u64))
                        .unwrap();
                }

                Ok(())
            }
            .boxed()
        })
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use crate::logging::test_utils::new_test_logger;

    use super::{rpc::MockEthRpcApi, *};
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
        // same, but HTTP
        assert_eq!(
            redact_secret_eth_node_endpoint(
                "https://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.rpc.rivet.cloud/"
            )
            .unwrap(),
            "https://cdc****.rinkeby.rpc.rivet.cloud/"
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
        // same, but HTTP
        assert_eq!(
            redact_secret_eth_node_endpoint("http://a").unwrap(),
            "http://a****"
        );
        assert!(redact_secret_eth_node_endpoint("no.schema.com").is_err());
    }
}
