use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use cf_chains::Ethereum;
use tokio::sync::Mutex;
use tracing::{info_span, Instrument};

use super::{
	rpc::EthDualRpcClient, safe_dual_block_subscription_from, witnessing::AllWitnessers,
	EthNumberBloom,
};
use crate::{
	constants::{BLOCK_PULL_TIMEOUT_MULTIPLIER, ETH_AVERAGE_BLOCK_TIME_SECONDS},
	multisig::PersistentKeyDB,
	stream_utils::EngineStreamExt,
	witnesser::{
		block_witnesser::{BlockWitnesser, BlockWitnesserWrapper},
		checkpointing::{get_witnesser_start_block_with_checkpointing, StartCheckpointing},
		epoch_witnesser::{start_epoch_witnesser, EpochWitnesserGenerator, WitnesserAndStream},
		EpochStart,
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

struct EthBlockWitnesser {
	epoch: EpochStart<Ethereum>,
}

#[async_trait]
impl BlockWitnesser for EthBlockWitnesser {
	type Chain = Ethereum;
	type Block = EthNumberBloom;
	type StaticState = AllWitnessers;

	async fn process_block(
		&mut self,
		block: EthNumberBloom,
		witnessers: &mut AllWitnessers,
	) -> anyhow::Result<()> {
		tracing::trace!("Eth block witnessers are processing block {}", block.block_number);

		futures::future::join_all([
			witnessers.key_manager.process_block(&self.epoch, &block),
			witnessers.stake_manager.process_block(&self.epoch, &block),
			witnessers.eth_ingress.process_block(&self.epoch, &block),
			witnessers.flip_ingress.process_block(&self.epoch, &block),
			witnessers.usdc_ingress.process_block(&self.epoch, &block),
		])
		.await
		.into_iter()
		.collect::<anyhow::Result<Vec<()>>>()
		.map_err(|err| {
			tracing::error!("Eth witnesser failed to process block: {err}");
			err
		})?;

		Ok(())
	}
}

struct EthBlockWitnesserGenerator {
	db: Arc<PersistentKeyDB>,
	eth_dual_rpc: EthDualRpcClient,
}

#[async_trait]
impl EpochWitnesserGenerator for EthBlockWitnesserGenerator {
	type Witnesser = BlockWitnesserWrapper<EthBlockWitnesser>;
	async fn init(
		&mut self,
		epoch: EpochStart<Ethereum>,
		// Why is this result option
	) -> anyhow::Result<Option<WitnesserAndStream<Self::Witnesser>>> {
		let (from_block, witnessed_until_sender) =
			match get_witnesser_start_block_with_checkpointing::<cf_chains::Ethereum>(
				epoch.epoch_index,
				epoch.block_number,
				self.db.clone(),
			)
			.await
			.expect("Failed to start Eth witnesser checkpointing")
			{
				StartCheckpointing::Started((from_block, witnessed_until_sender)) =>
					(from_block, witnessed_until_sender),
				StartCheckpointing::AlreadyWitnessedEpoch => return Ok(None),
			};

		let block_stream = safe_dual_block_subscription_from(from_block, self.eth_dual_rpc.clone())
			.await
			.map_err(|err| {
				tracing::error!("Subscription error: {err}");
				err
			})?
			.timeout_after(Duration::from_secs(
				ETH_AVERAGE_BLOCK_TIME_SECONDS * BLOCK_PULL_TIMEOUT_MULTIPLIER,
			));

		Ok(Some((
			BlockWitnesserWrapper {
				epoch_index: epoch.epoch_index,
				witnesser: EthBlockWitnesser { epoch },
				witnessed_until_sender,
			},
			Box::pin(block_stream),
		)))
	}
}

pub async fn start(
	epoch_start_receiver: Arc<Mutex<async_broadcast::Receiver<EpochStart<Ethereum>>>>,
	witnessers: AllWitnessers,
	eth_dual_rpc: EthDualRpcClient,
	db: Arc<PersistentKeyDB>,
) -> Result<(), ()> {
	start_epoch_witnesser(
		epoch_start_receiver,
		EthBlockWitnesserGenerator { db, eth_dual_rpc },
		witnessers,
	)
	.instrument(info_span!("Eth-Block-Head-Witnesser"))
	.await
}
