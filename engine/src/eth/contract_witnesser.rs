use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::broadcast;

use crate::{
    eth::rpc::EthDualRpcClient,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
};

use super::{
    epoch_witnesser::should_end_witnessing,
    rpc::{EthHttpRpcClient, EthWsRpcClient},
    EpochStart, EthContractWitnesser,
};

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start<ContractWitnesser, StateChainRpc>(
    contract_witnesser: ContractWitnesser,
    eth_ws_rpc: EthWsRpcClient,
    eth_http_rpc: EthHttpRpcClient,
    epoch_starts_receiver: broadcast::Receiver<EpochStart>,
    // In some cases there is no use witnessing older epochs since any actions that could be taken either have already
    // been taken, or can no longer be taken.
    witness_historical_epochs: bool,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    ContractWitnesser: 'static + EthContractWitnesser + Sync + Send,
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
{
    let contract_witnesser = Arc::new(contract_witnesser);

    super::epoch_witnesser::start(
        contract_witnesser.contract_name(),
        epoch_starts_receiver,
        move |epoch_start| witness_historical_epochs || epoch_start.current,
        (),
        move |end_witnessing_signal, epoch_start, (), logger| {
            let eth_ws_rpc = eth_ws_rpc.clone();
            let eth_http_rpc = eth_http_rpc.clone();
            let dual_rpc = EthDualRpcClient::new(eth_ws_rpc.clone(), eth_http_rpc.clone(), &logger);
            let contract_witnesser = contract_witnesser.clone();
            let state_chain_client = state_chain_client.clone();

            async move {
                let mut block_stream = contract_witnesser
                    .block_stream(eth_ws_rpc, eth_http_rpc, epoch_start.eth_block, &logger)
                    .await?;

                // TOOD: Handle None on stream, and result event being an error
                while let Some(block) = block_stream.next().await {
                    if should_end_witnessing(
                        end_witnessing_signal.clone(),
                        block.block_number,
                        &logger,
                    ) {
                        break;
                    }

                    for event in block.events {
                        contract_witnesser
                            .handle_event(
                                epoch_start.index,
                                block.block_number,
                                event,
                                state_chain_client.clone(),
                                &dual_rpc,
                                &logger,
                            )
                            .await?;
                    }
                }

                Ok(())
            }
        },
        logger,
    )
    .await
}
