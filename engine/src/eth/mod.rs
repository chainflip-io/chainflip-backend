mod http_safe_stream;
pub mod key_manager;
pub mod stake_manager;

pub mod event_common;

mod ws_safe_stream;

pub mod rpc;

pub mod utils;

use anyhow::{Context, Result};
use cf_traits::EpochIndex;
use regex::Regex;

use crate::{
    common::{make_periodic_tick, read_clean_and_decode_hex_str_file},
    constants::{
        ETH_BLOCK_SAFETY_MARGIN, ETH_FALLING_BEHIND_MARGIN_BLOCKS,
        ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL, ETH_STILL_BEHIND_LOG_INTERVAL,
    },
    eth::{
        http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL},
        rpc::EthWsRpcApi,
        ws_safe_stream::safe_ws_head_stream,
    },
    logging::{COMPONENT_KEY, ETH_HTTP_STREAM_YIELDED, ETH_STREAM_BEHIND, ETH_WS_STREAM_YIELDED},
    settings,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};
use ethbloom::{Bloom, Input};
use futures::{stream, FutureExt, StreamExt};
use slog::o;
use sp_core::{H160, U256};
use std::{
    cmp::Ordering,
    fmt::{self, Debug},
    pin::Pin,
    str::FromStr,
    sync::{Arc, Mutex},
};
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};
use web3::{
    ethabi::{self, Address, Contract, Event},
    signing::{Key, SecretKeyRef},
    types::{
        Block, BlockNumber, Bytes, CallRequest, FilterBuilder, TransactionParameters, H2048, H256,
        U64,
    },
};
use web3_secp256k1::SecretKey;

use tokio_stream::Stream;

use event_common::EventWithCommon;

use async_trait::async_trait;

#[derive(Debug, PartialEq)]
pub struct EthNumberBloom {
    pub block_number: U64,
    pub logs_bloom: H2048,
    pub base_fee_per_gas: U256,
}

use self::rpc::{EthHttpRpcClient, EthRpcApi, EthWsRpcClient};

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

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ObserveInstruction {
    Start(
        <cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber,
        EpochIndex,
    ),
    End(<cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber),
}

// TODO: Look at refactoring this to take specific "start" and "end" blocks, rather than this being implicit over the windows
// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start_contract_observer<ContractObserver, StateChainRpc>(
    contract_observer: ContractObserver,
    eth_ws_rpc: &EthWsRpcClient,
    eth_http_rpc: &EthHttpRpcClient,
    mut instruction_receiver: UnboundedReceiver<ObserveInstruction>,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    logger: &slog::Logger,
) where
    ContractObserver: 'static + EthObserver + Sync + Send,
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
{
    let logger =
        logger.new(o!(COMPONENT_KEY => format!("{}-Observer", contract_observer.contract_name())));
    slog::info!(logger, "Starting");

    type TaskEndBlock = Arc<Mutex<Option<u64>>>;

    let mut option_handle_end_block: Option<(JoinHandle<()>, TaskEndBlock)> = None;

    let contract_observer = Arc::new(contract_observer);

    while let Some(observe_instruction) = instruction_receiver.recv().await {
        match observe_instruction {
            ObserveInstruction::End(to_block) => {
                let (handle, end_at_block) = option_handle_end_block
                    .take()
                    .expect("Received two 'end' events in a row. This should not occur.");
                // We already have a thread, we want to tell it when to stop and await on it
                *end_at_block.lock().unwrap() = Some(to_block);
                handle.await.unwrap();
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
                    let logger = logger.clone();
                    let contract_observer = contract_observer.clone();
                    let state_chain_client = state_chain_client.clone();
                    (
                        tokio::spawn(async move {
                            slog::info!(logger, "Start observing from ETH block: {}", from_block);
                            let mut event_stream = contract_observer
                                .event_stream(eth_ws_rpc, eth_http_rpc, from_block, &logger)
                                .await
                                .expect("Failed to initialise event stream");

                            // TOOD: Handle None on stream, and result event being an error
                            while let Some(event) = event_stream.next().await {
                                if let Some(end_at_block) = *task_end_at_block.lock().unwrap() {
                                    // TODO: Have the stream end when the safe head gets to the block number,
                                    // not just when we receive an event (which could be arbitrarily far in the future)
                                    // past our window_to
                                    if event.block_number >= end_at_block {
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
                                    .handle_event(epoch, event, state_chain_client.clone(), &logger)
                                    .await;
                            }
                        }),
                        task_end_at_block_c,
                    )
                })
            }
        }
    }
}

impl TryFrom<Block<H256>> for EthNumberBloom {
    type Error = anyhow::Error;

    fn try_from(block: Block<H256>) -> Result<Self, Self::Error> {
        if block.number.is_none() || block.logs_bloom.is_none() || block.base_fee_per_gas.is_none()
        {
            Err(anyhow::Error::msg(
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
        let mut tx_params = TransactionParameters {
            to: Some(unsigned_tx.contract),
            data: unsigned_tx.data.clone().into(),
            chain_id: Some(unsigned_tx.chain_id),
            value: unsigned_tx.value,
            transaction_type: Some(web3::types::U64::from(2u64)),
            // Set the gas really high (~half gas in a block) for the estimate, since the estimation call requires you to
            // input at least as much gas as the estimate will return (stupid? yes)
            gas: U256::from(15_000_000u64),
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
        let uint256_2 = U256::from(2u64);
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
            .context("Failed to broadcast ETH transaction to network")
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
pub struct BlockEvents<EventParameters: Debug> {
    pub block_number: u64,
    pub events: Result<Vec<EventWithCommon<EventParameters>>>,
}

/// Just contains an empty vec if there are no events
#[derive(Debug)]
pub struct CleanBlockEvents<EventParameters: Debug> {
    pub block_number: u64,
    pub events: Vec<EventWithCommon<EventParameters>>,
}

// Specify a default type for the mock
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
    ) -> Result<Pin<Box<dyn Stream<Item = BlockEvents<Self::EventParameters>> + Send + '_>>>
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

                            (block_number.as_u64(), header.base_fee_per_gas, result_logs)
                        }
                    })
                    .map(
                        move |(block_number, base_fee_per_gas, result_logs)| BlockEvents {
                            block_number,
                            events: result_logs.and_then(|logs| {
                                logs.into_iter()
                                    .map(
                                        |unparsed_log| -> Result<
                                            EventWithCommon<Self::EventParameters>,
                                            anyhow::Error,
                                        > {
                                            EventWithCommon::<Self::EventParameters>::new_from_unparsed_logs(
                                                &decode_log_fn,
                                                unparsed_log,
                                                base_fee_per_gas,
                                            )
                                        },
                                    )
                                    .collect::<Result<Vec<_>>>()
                            }),
                        },
                    );

                return Ok(Box::pin(events));
            }
        }
        Err(anyhow::Error::msg("No events in ETH safe head stream"))
    }

    /// Get an event stream for the contract, returning the stream only if the head of the stream is
    /// ahead of from_block (otherwise it will wait until we have reached from_block)
    async fn event_stream(
        &self,
        eth_ws_rpc: EthWsRpcClient,
        eth_http_rpc: EthHttpRpcClient,
        // usually the start of the validator's active window
        from_block: u64,
        logger: &slog::Logger,
        // This stream must be Send, so it can be used by the spawn
    ) -> Result<Pin<Box<dyn Stream<Item = EventWithCommon<Self::EventParameters>> + Send + '_>>>
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
    ) -> Result<Pin<Box<dyn Stream<Item = EventWithCommon<Self::EventParameters>> + Send + 'a>>>
    where
        BlockEventsStreamWs: Stream<Item = BlockEvents<Self::EventParameters>> + Unpin + Send + 'a,
        BlockEventsStreamHttp:
            Stream<Item = BlockEvents<Self::EventParameters>> + Unpin + Send + 'a,
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
                log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL),
                protocol: TransportProtocol::Ws,
            },
            ws_stream: safe_ws_block_events_stream,
            http_state: ProtocolState {
                last_block_pulled: 0,
                log_ticker: make_periodic_tick(ETH_STILL_BEHIND_LOG_INTERVAL),
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
            BlockEventsStream: Stream<Item = BlockEvents<EventParameters>> + Unpin,
            EventParameters: Debug,
        >(
            merged_stream_state: &mut MergedStreamState,
            protocol_state: &mut ProtocolState,
            other_protocol_state: &mut ProtocolState,
            mut other_protocol_stream: BlockEventsStream,
            block_events: BlockEvents<EventParameters>,
        ) -> Result<Option<CleanBlockEvents<EventParameters>>> {
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
                match block_events.events {
                    Ok(events) => {
                        // yield, if we are at high enough block number
                        log_when_yielding(
                            protocol_state,
                            other_protocol_state,
                            merged_stream_state,
                            block_events.block_number,
                        );
                        Ok(Some(CleanBlockEvents {
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
                                    match block_events.events {
                                        Ok(events) => {
                                            log_when_yielding(
                                                other_protocol_state,
                                                protocol_state,
                                                merged_stream_state,
                                                block_events.block_number,
                                            );
                                            return Ok(Some(CleanBlockEvents {
                                                block_number: block_events.block_number,
                                                events,
                                            }));
                                        }
                                        Err(err) => {
                                            return Err(anyhow::Error::msg(format!("ETH {} stream failed with error, on block {} that we were recovering from: {}", other_protocol_state.protocol, block_events.block_number, err)));
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

                        return Err(anyhow::Error::msg(format!(
                            "ETH {} stream terminated when attempting to recover",
                            other_protocol_state.protocol,
                        )));
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
                                stream_state.merged_stream_state.last_block_yielded = clean_block_events.block_number;
                                break Some((stream::iter(clean_block_events.events), stream_state));
                            }
                        }
                        Err(err) => {
                            slog::error!(
                                stream_state.merged_stream_state.logger,
                                "Terminating ETH merged event stream due to error: {}",
                                err
                            );
                            break None;
                        }
                    }
                }
            },
        ).flatten()))
    }

    fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>>;

    async fn handle_event<RpcClient>(
        &self,
        epoch: EpochIndex,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        logger: &slog::Logger,
    ) where
        RpcClient: 'static + StateChainRpcApi + Sync + Send;

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

fn redact_and_log_node_endpoint(
    endpoint: &str,
    protocol: TransportProtocol,
    logger: &slog::Logger,
) {
    match redact_secret_eth_node_endpoint(endpoint) {
        Ok(redacted) => {
            slog::debug!(
                logger,
                "Connecting new {} web3 client to {}",
                protocol,
                redacted
            );
        }
        Err(e) => {
            slog::error!(
                logger,
                "Could not redact secret in {} ETH node endpoint: {}",
                protocol,
                e
            );
            slog::debug!(logger, "Connecting new {} web3 client", protocol);
        }
    }
}

#[cfg(test)]
mod merged_stream_tests {
    use std::time::Duration;

    use crate::logging::test_utils::new_test_logger;
    use crate::logging::test_utils::new_test_logger_with_tag_cache;
    use crate::logging::ETH_WS_STREAM_YIELDED;

    use super::key_manager::KeyManagerEvent;
    use super::{key_manager::ChainflipKey, rpc::mocks::MockEthHttpRpcClient};

    use super::key_manager::KeyManager;

    use super::*;

    // Arbitrariily chosen one of the EthRpc's for these tests
    fn test_km_contract() -> KeyManager<MockEthHttpRpcClient> {
        let mock_eth_http_rpc_client = MockEthHttpRpcClient::new();
        KeyManager::new(H160::default(), mock_eth_http_rpc_client).unwrap()
    }

    fn key_change(block_number: u64, log_index: u8) -> EventWithCommon<KeyManagerEvent> {
        EventWithCommon::<KeyManagerEvent> {
            tx_hash: Default::default(),
            log_index: U256::from(log_index),
            base_fee_per_gas: U256::from(2),
            block_number,
            event_parameters: KeyManagerEvent::AggKeySetByAggKey {
                old_key: ChainflipKey::default(),
                new_key: ChainflipKey::default(),
            },
        }
    }

    fn block_events_with_event(
        block_number: u64,
        log_indices: Vec<u8>,
    ) -> BlockEvents<KeyManagerEvent> {
        BlockEvents {
            block_number,
            events: Ok(log_indices
                .into_iter()
                .map(|index| key_change(block_number, index))
                .collect()),
        }
    }

    fn block_events_error(block_number: u64) -> BlockEvents<KeyManagerEvent> {
        BlockEvents {
            block_number,
            events: Err(anyhow::Error::msg("NOOOO")),
        }
    }

    fn block_events_no_events(block_number: u64) -> BlockEvents<KeyManagerEvent> {
        BlockEvents {
            block_number,
            events: Ok(vec![]),
        }
    }

    // Generate a stream for each protocol, that, when selected upon, will return
    // in the order the items are passed in
    // This is useful to test more "real world" scenarios, as stream::iter will always
    // immediately yield, therefore items will always be pealed off the streams
    // alternatingly
    fn interleaved_streams(
        // contains the streams in the order they will return
        items: Vec<(BlockEvents<KeyManagerEvent>, TransportProtocol)>,
    ) -> (
        // ws
        impl Stream<Item = BlockEvents<KeyManagerEvent>>,
        // http
        impl Stream<Item = BlockEvents<KeyManagerEvent>>,
    ) {
        assert!(!items.is_empty(), "should have at least one item");

        const DELAY_DURATION_MILLIS: u64 = 50;

        let mut protocol_last_returned = items.first().unwrap().1;
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

        let delayed_stream = |items: Vec<(BlockEvents<KeyManagerEvent>, Duration)>| {
            let items = items.into_iter();
            Box::pin(
                stream::unfold(items, |mut items| async move {
                    if let Some((i, d)) = items.next() {
                        tokio::time::sleep(d).await;
                        Some((i, items))
                    } else {
                        None
                    }
                })
                .fuse(),
            )
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
    async fn merged_stream_handles_multiple_events_in_same_block() {
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
        assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_YIELDED));
        tag_cache.clear();

        assert_eq!(stream.next().await.unwrap(), key_change(12, 0));
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_YIELDED));
        tag_cache.clear();

        assert_eq!(stream.next().await.unwrap(), key_change(15, 0));
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_YIELDED));
        tag_cache.clear();

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn merged_stream_notifies_once_every_x_blocks_when_one_falls_behind() {
        let (logger, tag_cache) = new_test_logger_with_tag_cache();

        let ws_range = 10..54;
        let range_block_events = ws_range
            .clone()
            .map(|n| block_events_with_event(n, vec![0]));

        let ws_stream = stream::iter(range_block_events);
        let http_stream = stream::iter([block_events_with_event(10, vec![0])]);

        let key_manager = test_km_contract();
        let mut stream = key_manager
            .merged_block_events_stream(ws_stream, http_stream, logger)
            .await
            .unwrap();

        for i in ws_range {
            let event = stream.next().await.unwrap();
            assert_eq!(event, key_change(i, 0));
        }

        assert_eq!(tag_cache.get_tag_count(ETH_STREAM_BEHIND), 4);
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    #[should_panic]
    async fn merged_stream_terminates_if_a_stream_moves_backwards() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let ws_stream = Box::pin(stream::iter([
            block_events_with_event(12, vec![0]),
            block_events_no_events(13),
            block_events_with_event(14, vec![2]),
            // We jump back here
            block_events_no_events(13),
            block_events_no_events(15),
            block_events_with_event(16, vec![0]),
        ]));

        let http_stream = Box::pin(stream::iter([
            block_events_with_event(12, vec![0]),
            block_events_no_events(13),
            block_events_with_event(14, vec![2]),
            // We jump back here
            block_events_no_events(13),
            block_events_no_events(15),
            block_events_with_event(16, vec![0]),
        ]));

        let mut stream = key_manager
            .merged_block_events_stream(ws_stream, http_stream, logger)
            .await
            .unwrap();

        stream.next().await.unwrap();
        stream.next().await.unwrap();
        stream.next().await.unwrap();
    }

    #[tokio::test]
    async fn merged_stream_recovers_when_one_stream_errors_and_other_catches_up_with_success() {
        let items = vec![
            (block_events_no_events(5), TransportProtocol::Http),
            (block_events_with_event(6, vec![0]), TransportProtocol::Http),
            (block_events_no_events(7), TransportProtocol::Http),
            (block_events_no_events(8), TransportProtocol::Http),
            (block_events_no_events(9), TransportProtocol::Http),
            // we had some events, but they are an error
            (block_events_error(10), TransportProtocol::Http),
            // so now we should enter recovery on the websockets stream
            (block_events_no_events(5), TransportProtocol::Ws),
            (block_events_with_event(6, vec![0]), TransportProtocol::Ws),
            (block_events_no_events(7), TransportProtocol::Ws),
            (block_events_no_events(8), TransportProtocol::Ws),
            (block_events_no_events(9), TransportProtocol::Ws),
            (block_events_with_event(10, vec![4]), TransportProtocol::Ws),
        ];

        let (logger, mut tag_cache) = new_test_logger_with_tag_cache();
        let (ws_stream, http_stream) = interleaved_streams(items);

        let key_manager = test_km_contract();
        let mut stream = key_manager
            .merged_block_events_stream(ws_stream, http_stream, logger)
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(6, 0));
        assert!(tag_cache.contains_tag(ETH_HTTP_STREAM_YIELDED));
        tag_cache.clear();

        assert_eq!(stream.next().await.unwrap(), key_change(10, 4));
        assert!(tag_cache.contains_tag(ETH_WS_STREAM_YIELDED));
        tag_cache.clear();

        assert!(stream.next().await.is_none());
    }

    // // handle when one of the streams doesn't even start - we won't get notified of that currently

    #[tokio::test]
    async fn merged_stream_exits_when_both_streams_have_error_events_for_a_block() {
        let key_manager = test_km_contract();
        let logger = new_test_logger();

        let ws_stream = Box::pin(stream::iter([
            block_events_with_event(11, vec![0]),
            block_events_error(12),
        ]));

        let http_stream = Box::pin(stream::iter([
            block_events_with_event(11, vec![0]),
            block_events_error(12),
        ]));

        let mut stream = key_manager
            .merged_block_events_stream(ws_stream, http_stream, logger)
            .await
            .unwrap();

        assert_eq!(stream.next().await.unwrap(), key_change(11, 0));

        assert!(stream.next().await.is_none());
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
