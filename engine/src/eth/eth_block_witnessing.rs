use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::Ethereum;
use futures::StreamExt;
use tokio::{select, sync::Mutex};
use tracing::{info, info_span, trace, Instrument};

use super::{
	rpc::EthDualRpcClient, safe_dual_block_subscription_from, witnessing::AllWitnessers,
	EthNumberBloom,
};
use crate::{
	multisig::{ChainTag, PersistentKeyDB},
	witnesser::{
		checkpointing::{
			get_witnesser_start_block_with_checkpointing, StartCheckpointing, WitnessedUntil,
		},
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

pub async fn start(
	epoch_start_receiver: Arc<Mutex<async_broadcast::Receiver<EpochStart<Ethereum>>>>,
	witnessers: AllWitnessers,
	eth_rpc: EthDualRpcClient,
	db: Arc<PersistentKeyDB>,
) -> Result<(), ()> {
	epoch_witnesser::start(
		epoch_start_receiver,
		move |_| true,
		witnessers,
		move |mut end_witnessing_receiver, epoch, mut witnessers| {
			let eth_rpc = eth_rpc.clone();
			let db = db.clone();
			async move {
				let (from_block, witnessed_until_sender) =
					match get_witnesser_start_block_with_checkpointing::<cf_chains::Ethereum>(
						ChainTag::Ethereum,
						epoch.epoch_index,
						epoch.block_number,
						db,
					)
					.await
					.expect("Failed to start Eth witnesser checkpointing")
					{
						StartCheckpointing::Started((from_block, witnessed_until_sender)) =>
							(from_block, witnessed_until_sender),
						StartCheckpointing::AlreadyWitnessedEpoch => return Ok(witnessers),
					};

				let mut block_stream =
					safe_dual_block_subscription_from(from_block, eth_rpc.clone()).await.map_err(
						|err| {
							tracing::error!("Subscription error: {err}");
						},
					)?;

				let mut end_at_block = None;
				let mut current_block = from_block;

				loop {
					let block = select! {
						end_block = &mut end_witnessing_receiver => {
							end_at_block = Some(end_block.expect("end witnessing channel was dropped unexpectedly"));
							None
						}
						Some(block) = block_stream.next() => {
							current_block = block.block_number.as_u64();
							Some(block)
						}
					};

					if let Some(end_block) = end_at_block {
						if current_block >= end_block {
							info!("Eth block witnessers unsubscribe at block {end_block}");
							break
						}
					}

					if let Some(block) = block {
						let block_number = block.block_number.as_u64();
						trace!("Eth block witnessers are processing block {block_number}");

						futures::future::join_all([
							witnessers.key_manager.process_block(&epoch, &block),
							witnessers.stake_manager.process_block(&epoch, &block),
							witnessers.eth_ingress.process_block(&epoch, &block),
							witnessers.flip_ingress.process_block(&epoch, &block),
							witnessers.usdc_ingress.process_block(&epoch, &block),
						])
						.await
						.into_iter()
						.collect::<anyhow::Result<Vec<()>>>()
						.map_err(|err| {
							tracing::error!("Witnesser failed to process block: {err}");
						})?;

						witnessed_until_sender
							.send(WitnessedUntil { epoch_index: epoch.epoch_index, block_number })
							.await
							.unwrap();
					}
				}

				Ok(witnessers)
			}
		},
	)
	.instrument(info_span!("Eth-Block-Head-Witnesser"))
	.await
}
