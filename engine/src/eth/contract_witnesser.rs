use std::{io::Write, sync::Arc, time::Duration};

use cf_primitives::EpochIndex;
use futures::{FutureExt, StreamExt};
use tokio::sync::broadcast::{self};

use crate::{
	eth::rpc::EthDualRpcClient, state_chain_observer::client::extrinsic_api::ExtrinsicApi,
};

use super::{
	block_events_stream_for_contract_from, epoch_witnesser::should_end_witnessing, EpochStart,
	EthContractWitnesser,
};

// NB: This code can emit the same witness multiple times. e.g. if the CFE restarts in the middle of
// witnessing a window of blocks
pub async fn start<StateChainClient, ContractWitnesser>(
	contract_witnesser: ContractWitnesser,
	eth_dual_rpc: EthDualRpcClient,
	epoch_starts_receiver: broadcast::Receiver<EpochStart>,
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
	use serde::{Deserialize, Serialize};

	#[derive(Clone, Debug, Serialize, Deserialize)]
	struct WitnessedUntil {
		epoch_index: EpochIndex,
		block_number: u64,
	}

	super::epoch_witnesser::start(
		contract_witnesser.contract_name(),
		epoch_starts_receiver,
		move |epoch_start| witness_historical_epochs || epoch_start.current,
		contract_witnesser,
		move |end_witnessing_signal, epoch_start, mut contract_witnesser, logger| {
			let state_chain_client = state_chain_client.clone();
			let eth_dual_rpc = eth_dual_rpc.clone();

			async move {
				let contract_name = contract_witnesser.contract_name();

				let witnessed_until = tokio::task::spawn_blocking({
					let contract_name = contract_name.clone();
					move || match std::fs::read_to_string(&contract_name)
						.map_err(anyhow::Error::new)
						.and_then(|string| {
							serde_json::from_str::<WitnessedUntil>(&string)
								.map_err(anyhow::Error::new)
						}) {
						Ok(witnessed_record) => witnessed_record,
						Err(_) => WitnessedUntil { epoch_index: 0, block_number: 0 },
					}
				})
				.await
				.unwrap();

				let (witnessed_until_sender, witnessed_until_receiver) =
					tokio::sync::watch::channel(witnessed_until.clone());

				tokio::task::spawn_blocking({
					let contract_name = contract_name.clone();
					move || loop {
						std::thread::sleep(Duration::from_secs(4));
						if let Ok(changed) = witnessed_until_receiver.has_changed() {
							if changed {
								atomicwrites::AtomicFile::new(
									&contract_name,
									atomicwrites::OverwriteBehavior::AllowOverwrite,
								)
								.write(|file| {
									write!(
										file,
										"{}",
										serde_json::to_string::<WitnessedUntil>(
											&witnessed_until_receiver.borrow()
										)
										.unwrap()
									)
								});
							}
						} else {
							break
						}
					}
				});

				if epoch_start.epoch_index >= witnessed_until.epoch_index {
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
							break
						}

						contract_witnesser
							.handle_block_events(
								epoch_start.epoch_index,
								if witnessed_until.epoch_index == epoch_start.epoch_index {
									std::cmp::max(block.block_number, witnessed_until.block_number)
								} else {
									block.block_number
								},
								block,
								state_chain_client.clone(),
								&eth_dual_rpc,
								&logger,
							)
							.await?;

						witnessed_until_sender
							.send(WitnessedUntil {
								epoch_index: epoch_start.epoch_index,
								block_number: epoch_start.eth_block,
							})
							.unwrap();
					}
				}
				Ok(contract_witnesser)
			}
		},
		logger,
	)
	.await
}
