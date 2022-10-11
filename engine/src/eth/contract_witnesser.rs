use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::broadcast::{self};

use crate::{
    eth::rpc::EthDualRpcClient,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
};

use super::{
    block_events_stream_for_contract_from, epoch_witnesser::should_end_witnessing, EpochStart,
    EthContractWitnesser,
};

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start<ContractWitnesser, StateChainRpc>(
    contract_witnesser: ContractWitnesser,
    eth_dual_rpc: EthDualRpcClient,
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
    super::epoch_witnesser::start(
        contract_witnesser.contract_name(),
        epoch_starts_receiver,
        move |epoch_start| witness_historical_epochs || epoch_start.current,
        contract_witnesser,
        move |end_witnessing_signal, epoch_start, mut contract_witnesser, logger| {
            let state_chain_client = state_chain_client.clone();
            let eth_dual_rpc = eth_dual_rpc.clone();

            async move {
                let mut block_stream = block_events_stream_for_contract_from(
                    epoch_start.eth_block,
                    &contract_witnesser,
                    eth_dual_rpc.clone(),
                    &logger,
                )
                .await?;

                while let Some(block) = block_stream.next().await {
                    if should_end_witnessing(
                        end_witnessing_signal.clone(),
                        block.block_number,
                        &logger,
                    ) {
                        break;
                    }

                    contract_witnesser
                        .handle_block_events(
                            epoch_start.index,
                            block.block_number,
                            block,
                            state_chain_client.clone(),
                            &eth_dual_rpc,
                            &logger,
                        )
                        .await?;
                }
                Ok(contract_witnesser)
            }
        },
        logger,
    )
    .await
}
