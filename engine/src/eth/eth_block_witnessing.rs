use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use cf_chains::Ethereum;
use futures::TryStreamExt;
use tokio::sync::Mutex;
use tracing::{info_span, Instrument};

use super::{
	rpc::{EthHttpRpcClient, EthWsRpcClient},
	safe_block_subscription_from,
	witnessing::AllWitnessers,
	EthNumberBloom,
};
use crate::{
	constants::{BLOCK_PULL_TIMEOUT_MULTIPLIER, ETH_AVERAGE_BLOCK_TIME_SECONDS},
	db::PersistentKeyDB,
	witnesser::{
		block_witnesser::{
			BlockStream, BlockWitnesser, BlockWitnesserGenerator, BlockWitnesserGeneratorWrapper,
		},
		epoch_process_runner::start_epoch_process_runner,
		ChainBlockNumber, EpochStart,
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
			witnessers.state_chain_gateway.process_block(&self.epoch, &block),
			witnessers.eth_deposits.process_block(&self.epoch, &block),
			witnessers.flip_deposits.process_block(&self.epoch, &block),
			witnessers.usdc_deposits.process_block(&self.epoch, &block),
			witnessers.vault.process_block(&self.epoch, &block),
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
	/// WS client for subscribing to new blocks
	ws_rpc: EthWsRpcClient,
	/// HTTP client for fetching any historical blocks
	http_rpc: EthHttpRpcClient,
}

#[async_trait]
impl BlockWitnesserGenerator for EthBlockWitnesserGenerator {
	type Witnesser = EthBlockWitnesser;

	fn create_witnesser(
		&self,
		epoch: EpochStart<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> Self::Witnesser {
		EthBlockWitnesser { epoch }
	}

	async fn get_block_stream(
		&mut self,
		from_block: ChainBlockNumber<Ethereum>,
	) -> anyhow::Result<BlockStream<EthNumberBloom>> {
		let block_stream =
			safe_block_subscription_from(from_block, self.ws_rpc.clone(), self.http_rpc.clone())
				.await
				.map_err(|err| {
					tracing::error!("Subscription error: {err}");
					err
				})?;
		let block_stream = tokio_stream::StreamExt::timeout(
			block_stream,
			Duration::from_secs(ETH_AVERAGE_BLOCK_TIME_SECONDS * BLOCK_PULL_TIMEOUT_MULTIPLIER),
		)
		.map_err(anyhow::Error::msg);

		Ok(Box::pin(block_stream))
	}
}

pub async fn start(
	epoch_start_receiver: Arc<Mutex<async_broadcast::Receiver<EpochStart<Ethereum>>>>,
	witnessers: AllWitnessers,
	ws_rpc: EthWsRpcClient,
	http_rpc: EthHttpRpcClient,
	db: Arc<PersistentKeyDB>,
) -> Result<(), ()> {
	start_epoch_process_runner(
		epoch_start_receiver,
		BlockWitnesserGeneratorWrapper {
			db,
			generator: EthBlockWitnesserGenerator { ws_rpc, http_rpc },
		},
		witnessers,
	)
	.instrument(info_span!("Eth-Block-Head-Witnesser"))
	.await
}
