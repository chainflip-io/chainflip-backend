use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::Ethereum;
use futures::StreamExt;

use super::{rpc::EthDualRpcClient, safe_dual_block_subscription_from, EthNumberBloom};
use crate::{
	multisig::{eth::EthSigning, PersistentKeyDB},
	witnesser::{
		checkpointing::{start_checkpointing_for, WitnessedUntil},
		epoch_witnesser, EpochStart,
	},
};

#[async_trait]
pub trait BlockProcessor: Send {
	async fn process_block(
		&mut self,
		epoch: &EpochStart<Ethereum>,
		block: &EthNumberBloom,
	) -> anyhow::Result<()>;
}

pub async fn start<const N: usize>(
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	eth_rpc: EthDualRpcClient,
	witnessers: [Box<dyn BlockProcessor>; N],
	db: Arc<PersistentKeyDB>,
	logger: &slog::Logger,
) -> anyhow::Result<()> {
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
					start_checkpointing_for::<EthSigning>("block-head", db, &logger);

				// Don't witness epochs that we've already witnessed
				if epoch.epoch_index < witnessed_until.epoch_index {
					return Ok(witnessers)
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
					safe_dual_block_subscription_from(from_block, eth_rpc.clone(), &logger).await?;

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

					futures::future::join_all(
						witnessers.iter_mut().map(|w| w.process_block(&epoch, &block)),
					)
					.await
					.into_iter()
					.collect::<anyhow::Result<Vec<()>>>()?;

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
		logger,
	)
	.await?;

	Ok(())
}
