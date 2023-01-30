use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::Ethereum;
use futures::StreamExt;
use sp_core::H160;

use super::{
	rpc::EthDualRpcClient, safe_dual_block_subscription_from, witnessing::AllWitnessers,
	EthNumberBloom,
};
use crate::{
	multisig::{ChainTag, PersistentKeyDB},
	state_chain_observer::client::StateChainClient,
	witnesser::{
		checkpointing::{start_checkpointing_for, WitnessedUntil},
		epoch_witnesser, EpochStart,
	},
use crate::witnesser::{
	checkpointing::{start_checkpointing_for, WitnessedUntil},
	epoch_witnesser, EpochStart,
};

pub struct IngressAddressReceivers {
	pub eth: tokio::sync::mpsc::UnboundedReceiver<H160>,
	pub flip: tokio::sync::mpsc::UnboundedReceiver<H160>,
	pub usdc: tokio::sync::mpsc::UnboundedReceiver<H160>,
}

#[async_trait]
pub trait BlockProcessor: Send {
	async fn process_block(
		&mut self,
		epoch: &EpochStart<Ethereum>,
		block: &EthNumberBloom,
	) -> anyhow::Result<()>;
}

pub async fn start(
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	eth_rpc: EthDualRpcClient,
	witnessers: AllWitnessers,
	db: Arc<PersistentKeyDB>,
	logger: slog::Logger,
) -> Result<(), (async_broadcast::Receiver<EpochStart<Ethereum>>, IngressAddressReceivers)> {
	match epoch_witnesser::start(
		"Block_Head".to_string(),
		epoch_start_receiver,
		move |_| true,
		witnessers,
		move |end_witnessing_signal, epoch, mut witnessers, logger| {
			let eth_rpc = eth_rpc.clone();
			let db = db.clone();
			async move {
				let (witnessed_until, witnessed_until_sender, _checkpointing_join_handle) =
					start_checkpointing_for(ChainTag::Ethereum, db, &logger);

				// Don't witness epochs that we've already witnessed
				if epoch.epoch_index < witnessed_until.epoch_index {
					return Result::<_, IngressAddressReceivers>::Ok(witnessers)
				}

				// We do this because it's possible to witness ahead of the epoch start during the
				// previous epoch. If we don't start witnessing from the epoch start, when we
				// receive a new epoch, we won't witness some of the blocks for the particular
				// epoch, since witness extrinsics are submitted with the epoch number it's for.
				let from_block = if witnessed_until.epoch_index == epoch.epoch_index {
					// Start where we left off
					witnessed_until.block_number
				} else {
					// We haven't witnessed this epoch yet, so start from the beginning
					epoch.block_number
				};

				let mut block_stream =
					match safe_dual_block_subscription_from(from_block, eth_rpc.clone(), &logger)
						.await
					{
						Ok(stream) => stream,
						Err(e) => {
							slog::error!(
								logger,
								"Eth block witnessers failed to subscribe to eth blocks: {:?}",
								e
							);
							return Err(IngressAddressReceivers {
								eth: witnessers.2.take_ingress_receiver(),
								flip: witnessers.3.take_ingress_receiver(),
								usdc: witnessers.4.take_ingress_receiver(),
							})
						},
					};

				while let Some(block) = block_stream.next().await {
					if let Some(end_block) = *end_witnessing_signal.lock().unwrap() {
						if block.block_number.as_u64() >= end_block {
							slog::info!(
								logger,
								"Eth block witnessers unsubscribe at block {}",
								end_block
							);
							break
						}
					}

					slog::trace!(
						logger,
						"Eth block witnessers are processing block {:?}",
						block.block_number
					);

					match futures::future::join_all([
						witnessers.0.process_block(&epoch, &block),
						witnessers.1.process_block(&epoch, &block),
						witnessers.2.process_block(&epoch, &block),
						witnessers.3.process_block(&epoch, &block),
						witnessers.4.process_block(&epoch, &block),
					])
					.await
					.into_iter()
					.collect::<anyhow::Result<Vec<()>>>()
					{
						Ok(_) => (),
						Err(e) => {
							slog::error!(
								logger,
								"Eth block witnessers failed to process block {:?}: {:?}",
								block.block_number,
								e
							);
							return Err(IngressAddressReceivers {
								eth: witnessers.2.take_ingress_receiver(),
								flip: witnessers.3.take_ingress_receiver(),
								usdc: witnessers.4.take_ingress_receiver(),
							})
						},
					}

					witnessed_until_sender
						.send(WitnessedUntil {
							epoch_index: epoch.epoch_index,
							block_number: block.block_number.as_u64(),
						})
						.unwrap();
				}

				Ok(witnessers)
			}
		},
		&logger,
	)
	.await
	{
		Ok(_) => Ok(()),
		Err((epoch_start_receiver, e)) => Err((epoch_start_receiver, e)),
	}
}
