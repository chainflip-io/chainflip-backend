use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::broadcast;

use crate::{
    eth::rpc::EthDualRpcClient,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
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
    epoch_starts_receiver: broadcast::Receiver<EpochStart>,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    ContractObserver: 'static + EthObserver + Sync + Send,
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
{
    let contract_observer = Arc::new(contract_observer);

    super::epoch_observer::start(
        format!("{}-Observer", contract_observer.contract_name()),
        state_chain_client,
        epoch_starts_receiver,
        |epoch_start| epoch_start.participant,
        (),
        move |state_chain_client, end_observation_signal, epoch_start, (), logger| {
            let eth_ws_rpc = eth_ws_rpc.clone();
            let eth_http_rpc = eth_http_rpc.clone();
            let dual_rpc = EthDualRpcClient::new(eth_ws_rpc.clone(), eth_http_rpc.clone(), &logger);
            let contract_observer = contract_observer.clone();

            async move {
                slog::info!(
                    logger,
                    "Start observing from ETH block: {}",
                    epoch_start.eth_block
                );
                let mut block_stream = contract_observer
                    .block_stream(eth_ws_rpc, eth_http_rpc, epoch_start.eth_block, &logger)
                    .await
                    .expect("Failed to initialise block stream");

                // TOOD: Handle None on stream, and result event being an error
                while let Some(block) = block_stream.next().await {
                    if let Some(end_block) = *end_observation_signal.lock().unwrap() {
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
            }
        },
        logger,
    )
    .await
}
