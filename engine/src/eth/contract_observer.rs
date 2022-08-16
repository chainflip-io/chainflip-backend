use std::sync::{Arc, Mutex};

use futures::{FutureExt, StreamExt};
use slog::o;
use tokio::sync::broadcast;

use crate::{
    eth::rpc::EthDualRpcClient,
    logging::COMPONENT_KEY,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
    task_scope::{with_task_scope, ScopedJoinHandle},
};

use super::{
    rpc::{EthHttpRpcClient, EthWsRpcClient},
    EpochStart, EthObserver,
};

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start<ContractObserver, StateChainRpc>(
    contract_observer: ContractObserver,
    eth_ws_rpc: EthWsRpcClient,
    eth_http_rpc: EthHttpRpcClient,
    mut epoch_starts_receiver: broadcast::Receiver<EpochStart>,
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

            let mut handle_and_end_observation_signal: Option<(
                ScopedJoinHandle<()>,
                Arc<Mutex<Option<u64>>>,
            )> = None;

            let contract_observer = Arc::new(contract_observer);

            while let Ok(epoch_start) = epoch_starts_receiver
                .recv()
                .await
                .map_err(|e| slog::error!(logger, "Epoch start receiver failed: {:?}", e))
            {
                if let Some((handle, end_observation_signal)) =
                    handle_and_end_observation_signal.take()
                {
                    *end_observation_signal.lock().unwrap() = Some(epoch_start.eth_block);
                    handle.await;
                }

                if epoch_start.participant {
                    handle_and_end_observation_signal = Some({
                        let end_observation_signal = Arc::new(Mutex::new(None));

                        // clone for capture by tokio task
                        let end_observation_signal_c = end_observation_signal.clone();
                        let eth_ws_rpc = eth_ws_rpc.clone();
                        let eth_http_rpc = eth_http_rpc.clone();
                        let dual_rpc =
                            EthDualRpcClient::new(eth_ws_rpc.clone(), eth_http_rpc.clone());
                        let logger = logger.clone();
                        let contract_observer = contract_observer.clone();
                        let state_chain_client = state_chain_client.clone();
                        (
                            scope.spawn_with_handle(async move {
                                slog::info!(
                                    logger,
                                    "Start observing from ETH block: {}",
                                    epoch_start.eth_block
                                );
                                let mut block_stream = contract_observer
                                    .block_stream(
                                        eth_ws_rpc,
                                        eth_http_rpc,
                                        epoch_start.eth_block,
                                        &logger,
                                    )
                                    .await
                                    .expect("Failed to initialise block stream");

                                // TOOD: Handle None on stream, and result event being an error
                                while let Some(block) = block_stream.next().await {
                                    if let Some(end_block) = *end_observation_signal.lock().unwrap()
                                    {
                                        if block.block_number >= end_block {
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
                                                epoch_start.index,
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
                            end_observation_signal_c,
                        )
                    })
                }
            }

            Ok(())
        }
        .boxed()
    })
    .await
}
