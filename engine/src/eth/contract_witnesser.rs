use std::sync::Arc;

use cf_chains::eth::Ethereum;
use futures::StreamExt;

use crate::{
	eth::rpc::EthDualRpcClient,
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{
		epoch_witnesser::{self, should_end_witnessing},
		EpochStart,
	},
};

use super::{block_events_stream_for_contract_from, EthContractWitnesser};

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of
// witnessing a window of blocks
pub async fn start<StateChainClient, ContractWitnesser>(
	contract_witnesser: ContractWitnesser,
	eth_dual_rpc: EthDualRpcClient,
	epoch_starts_receiver: async_channel::Receiver<EpochStart<Ethereum>>,
	// In some cases there is no use witnessing older epochs since any actions that could be taken
	// either have already been taken, or can no longer be taken.
	witness_historical_epochs: bool,
	state_chain_client: Arc<StateChainClient>,
	logger: &slog::Logger,
) -> anyhow::Result<()>
where
	ContractWitnesser: 'static + EthContractWitnesser + Sync + Send,
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	epoch_witnesser::start(
		contract_witnesser.contract_name(),
		epoch_starts_receiver,
		move |epoch_start| witness_historical_epochs || epoch_start.current,
		contract_witnesser,
		move |end_witnessing_signal, epoch_start, mut contract_witnesser, logger| {
			let state_chain_client = state_chain_client.clone();
			let eth_dual_rpc = eth_dual_rpc.clone();

			async move {
				let mut block_stream = block_events_stream_for_contract_from(
					epoch_start.block_number,
					&contract_witnesser,
					eth_dual_rpc.clone(),
					&logger,
				)
				.await?;

				while let Some(block) = block_stream.next().await {
					if should_end_witnessing::<Ethereum>(
						end_witnessing_signal.clone(),
						block.block_number,
						&logger,
					) {
						break
					}

					contract_witnesser
						.handle_block_events(
							epoch_start.epoch_index,
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
