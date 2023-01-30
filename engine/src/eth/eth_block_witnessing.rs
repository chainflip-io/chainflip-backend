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
	try_or_throw,
	witnesser::{
		checkpointing::{start_checkpointing_for, WitnessedUntil},
		epoch_witnesser, EpochStart,
	},
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
	witnessers: AllWitnessers,
	eth_rpc: EthDualRpcClient,
	db: Arc<PersistentKeyDB>,
	logger: slog::Logger,
) -> Result<(), (async_broadcast::Receiver<EpochStart<Ethereum>>, IngressAddressReceivers)> {
	epoch_witnesser::start(
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

				// We need to throw out the receivers so we can restart the process while ensuring
				// we are still able to receive new ingress addresses to monitor.
				macro_rules! try_or_throw_receivers {
					($exp:expr) => {
						try_or_throw!(
							$exp,
							IngressAddressReceivers {
								eth: witnessers.eth_ingress.take_ingress_receiver(),
								flip: witnessers.flip_ingress.take_ingress_receiver(),
								usdc: witnessers.usdc_ingress.take_ingress_receiver(),
							},
							&logger
						)
					};
				}

				let mut block_stream = try_or_throw_receivers!(
					safe_dual_block_subscription_from(from_block, eth_rpc.clone(), &logger).await
				);

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

					try_or_throw_receivers!(futures::future::join_all([
						witnessers.key_manager.process_block(&epoch, &block),
						witnessers.stake_manager.process_block(&epoch, &block),
						witnessers.eth_ingress.process_block(&epoch, &block),
						witnessers.flip_ingress.process_block(&epoch, &block),
						witnessers.usdc_ingress.process_block(&epoch, &block),
					])
					.await
					.into_iter()
					.collect::<anyhow::Result<Vec<()>>>());

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
}
