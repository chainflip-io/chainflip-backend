use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::broadcast::{self};

use crate::{
    eth::rpc::EthDualRpcClient,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
};

use super::{
    epoch_witnesser::should_end_witnessing,
    rpc::{EthHttpRpcClient, EthWsRpcClient},
    EpochStart, EthContractWitnesser,
};

/// Trait that alows state updates within a paritcular contract witnesser
#[async_trait]
pub trait ContractStateUpdate {
    // Item used for filtering the events.
    type Item: 'static + Send + Sync + Clone + Copy;

    /// Get the next item to use for updating the state.
    /// Defaults to a pending future, which is effeitively a no-op when used as a branch
    /// in a tokio_select! {}.
    fn next_item_to_update(
        &mut self,
    ) -> Pin<Box<dyn futures::Future<Output = Option<Self::Item>> + Send + '_>> {
        Box::pin(futures::future::pending())
    }

    /// Updates the inner state with the `new_item`.
    fn update_state(&mut self, _new_item: Self::Item) {
        // do nothing as a default
    }

    /// Checks if we should act on the item
    fn should_act_on(&self, _item: &Self::Item) -> bool {
        true
    }
}

pub struct NoContractState {}

impl ContractStateUpdate for NoContractState {
    type Item = ();
}

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of witnessing a window of blocks
pub async fn start<ContractWitnesser, StateChainRpc, ContractWitnesserState>(
    contract_witnesser: ContractWitnesser,
    eth_ws_rpc: EthWsRpcClient,
    eth_http_rpc: EthHttpRpcClient,
    epoch_starts_receiver: broadcast::Receiver<EpochStart>,
    // None for non-ERC20 contracts, contains the initial set of addresses to monitor otherwise, including a channel to provide
    // updates to this list.
    contract_witnesser_state: ContractWitnesserState,
    // In some cases there is no use witnessing older epochs since any actions that could be taken either have already
    // been taken, or can no longer be taken.
    witness_historical_epochs: bool,
    state_chain_client: Arc<StateChainClient<StateChainRpc>>,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    ContractWitnesser: 'static + EthContractWitnesser + Sync + Send,
    StateChainRpc: 'static + StateChainRpcApi + Sync + Send,
    ContractWitnesserState:
        'static + Send + Sync + ContractStateUpdate<Item = ContractWitnesser::StateItem>,
{
    let contract_witnesser = Arc::new(contract_witnesser);

    super::epoch_witnesser::start(
        contract_witnesser.contract_name(),
        epoch_starts_receiver,
        move |epoch_start| witness_historical_epochs || epoch_start.current,
        contract_witnesser_state,
        move |end_witnessing_signal, epoch_start, mut contract_witnesser_state, logger| {
            let eth_ws_rpc = eth_ws_rpc.clone();
            let eth_http_rpc = eth_http_rpc.clone();
            let dual_rpc = EthDualRpcClient::new(eth_ws_rpc.clone(), eth_http_rpc.clone(), &logger);
            let contract_witnesser = contract_witnesser.clone();
            let state_chain_client = state_chain_client.clone();

            async move {
                let mut block_stream = contract_witnesser
                    .block_stream(eth_ws_rpc, eth_http_rpc, epoch_start.eth_block, &logger)
                    .await?;

                loop {
                    tokio::select! {
                        biased;
                        Some(new_item) = contract_witnesser_state.next_item_to_update() => {
                            contract_witnesser_state.update_state(new_item);
                        },
                        Some(block) = block_stream.next() => {
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
                                        &contract_witnesser_state,
                                        state_chain_client.clone(),
                                        &dual_rpc,
                                        &logger,
                                    )
                                    .await?;
                            }
                        }
                    }
                }

                Ok(contract_witnesser_state)
            }
        },
        logger,
    )
    .await
}
