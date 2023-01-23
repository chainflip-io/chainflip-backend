use async_trait::async_trait;
use cf_chains::Ethereum;
use futures::StreamExt;

use super::{rpc::EthDualRpcClient, safe_dual_block_subscription_from, EthNumberBloom};
use crate::witnesser::{epoch_witnesser, EpochStart};

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
	logger: &slog::Logger,
) -> anyhow::Result<()> {
	epoch_witnesser::start(
		"Block_Head".to_string(),
		epoch_start_receiver,
		move |_| true,
		witnessers,
		move |end_witnessing_signal, epoch, mut witnessers, logger| {
			let eth_rpc = eth_rpc.clone();
			async move {
				let mut block_stream =
					safe_dual_block_subscription_from(epoch.block_number, eth_rpc.clone(), &logger)
						.await?;

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
				}

				Ok(witnessers)
			}
		},
		logger,
	)
	.await?;

	Ok(())
}
