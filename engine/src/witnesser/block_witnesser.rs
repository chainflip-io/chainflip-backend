use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use cf_chains::Chain;
use cf_primitives::EpochIndex;
use futures_util::TryStreamExt;
use multisig::ChainTag;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tracing::{error, instrument, Instrument};

use crate::{
	constants::{
		BLOCK_PULL_TIMEOUT_MULTIPLIER, BTC_AVERAGE_BLOCK_TIME_SECONDS,
		DOT_AVERAGE_BLOCK_TIME_SECONDS, ETH_AVERAGE_BLOCK_TIME_SECONDS,
	},
	db::PersistentKeyDB,
};

use super::{
	checkpointing::{
		get_witnesser_start_block_with_checkpointing, StartCheckpointing, WitnessedUntil,
	},
	epoch_process_runner::{self, EpochProcessGenerator, EpochWitnesser, WitnesserInitResult},
	ChainBlockNumber, EpochStart, HasBlockNumber, HasChainTag,
};

#[async_trait]
pub trait BlockWitnesser: Send + Sync + 'static {
	type Chain: cf_chains::Chain + HasChainTag;
	type Block: Send + HasBlockNumber<BlockNumber = ChainBlockNumber<Self::Chain>>;
	type StaticState: Send;

	async fn process_block(
		&mut self,
		block: Self::Block,
		state: &mut Self::StaticState,
	) -> anyhow::Result<()>;
}

/// Turns a block witnesser into an epoch witnesser by
/// implementing functionality shared by all block witnessers.
pub struct BlockWitnesserWrapper<W>
where
	W: BlockWitnesser,
{
	pub witnesser: W,
	pub epoch_index: EpochIndex,
	pub witnessed_until_sender: tokio::sync::mpsc::Sender<WitnessedUntil>,
}

#[async_trait]
impl<W> EpochWitnesser for BlockWitnesserWrapper<W>
where
	W: BlockWitnesser,
{
	type Chain = W::Chain;
	type Data = W::Block;
	type StaticState = W::StaticState;

	const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool = true;

	async fn run_witnesser(
		self,
		data_stream: std::pin::Pin<
			Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
		>,
		end_witnessing_receiver: oneshot::Receiver<ChainBlockNumber<Self::Chain>>,
		state: Self::StaticState,
	) -> Result<Self::StaticState, ()> {
		epoch_process_runner::run_witnesser_block_stream(
			self,
			data_stream,
			end_witnessing_receiver,
			state,
		)
		.await
	}

	#[instrument(level = "trace", skip_all, fields(chain = Self::Chain::NAME, block_number = block.block_number().into()))]
	async fn do_witness(
		&mut self,
		block: W::Block,
		state: &mut Self::StaticState,
	) -> anyhow::Result<()> {
		let block_number = block.block_number().into();

		self.witnesser
			.process_block(block, state)
			.instrument(tracing::trace_span!("process_block"))
			.await?;

		self.witnessed_until_sender
			.send(WitnessedUntil { epoch_index: self.epoch_index, block_number })
			.instrument(tracing::trace_span!("send_witnessed_until"))
			.await
			.unwrap();

		Ok(())
	}
}

pub type BlockStream<Block> =
	std::pin::Pin<Box<dyn futures::Stream<Item = anyhow::Result<Block>> + Send + 'static>>;

#[async_trait]
pub trait BlockWitnesserGenerator: Send {
	type Witnesser: BlockWitnesser;

	fn create_witnesser(
		&self,
		epoch: EpochStart<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> Self::Witnesser;

	async fn get_block_stream(
		&mut self,
		from_block: ChainBlockNumber<<Self::Witnesser as BlockWitnesser>::Chain>,
	) -> anyhow::Result<BlockStream<<Self::Witnesser as BlockWitnesser>::Block>>;
}

pub struct BlockWitnesserGeneratorWrapper<Generator>
where
	Generator: BlockWitnesserGenerator,
{
	pub generator: Generator,
	pub db: Arc<PersistentKeyDB>,
}

#[async_trait]
impl<Generator> EpochProcessGenerator for BlockWitnesserGeneratorWrapper<Generator>
where
	Generator: BlockWitnesserGenerator,
	<<<Generator::Witnesser as BlockWitnesser>::Chain as cf_chains::Chain>::ChainBlockNumber as TryFrom<u64>>::Error: std::fmt::Debug
{
	type Witnesser = BlockWitnesserWrapper<Generator::Witnesser>;

	async fn init(
		&mut self,
		epoch: EpochStart<<Generator::Witnesser as BlockWitnesser>::Chain>,
	) -> anyhow::Result<WitnesserInitResult<Self::Witnesser>> {
		let chain: &'static str = <Generator::Witnesser as BlockWitnesser>::Chain::NAME;
		let expected_block_time_seconds = match <<Generator::Witnesser as BlockWitnesser>::Chain as HasChainTag>::CHAIN_TAG {
			ChainTag::Ethereum => ETH_AVERAGE_BLOCK_TIME_SECONDS,
			ChainTag::Bitcoin => BTC_AVERAGE_BLOCK_TIME_SECONDS,
			ChainTag::Polkadot => DOT_AVERAGE_BLOCK_TIME_SECONDS,
			ChainTag::Ed25519 => panic!("Ed25519 witnesser does not exist."),
		};

		let (from_block, witnessed_until_sender) =
			match get_witnesser_start_block_with_checkpointing::<
				<Generator::Witnesser as BlockWitnesser>::Chain,
			>(epoch.epoch_index, epoch.block_number, self.db.clone())
			.await
			.unwrap_or_else(|_| panic!("Failed to start {chain} witnesser checkpointing"))
			{
				StartCheckpointing::Started((from_block, witnessed_until_sender)) =>
					(from_block, witnessed_until_sender),
				StartCheckpointing::AlreadyWitnessedEpoch => return Ok(WitnesserInitResult::EpochSkipped),
			};

		tracing::info!("{chain} block witnesser is starting from block {}", from_block);

		let block_stream = self.generator
			.get_block_stream(from_block)
			.await?
			.timeout(Duration::from_secs(expected_block_time_seconds * BLOCK_PULL_TIMEOUT_MULTIPLIER))
			.map(|timeout_result| {
				timeout_result.unwrap_or_else(|_| {
					let chain = <Generator::Witnesser as BlockWitnesser>::Chain::NAME;
					error!("{chain} block stream timed out.", );
					Err(anyhow::anyhow!("{chain} block stream timed out."))
				})
			})
			.map_err(|err| {
				let chain = <Generator::Witnesser as BlockWitnesser>::Chain::NAME;
				error!("Error while fetching {chain} events: {:?}", err);
				anyhow::anyhow!("Error while fetching {chain} events: {:?}", err)
			})
			.chain(futures_util::stream::once(async {
				let chain = <Generator::Witnesser as BlockWitnesser>::Chain::NAME;
				error!("{chain} block stream ended unexpectedly");
				Err(anyhow::anyhow!("{chain} block stream ended unexpectedly"))
			}));

		let witnesser = BlockWitnesserWrapper {
			epoch_index: epoch.epoch_index,
			witnesser: self.generator.create_witnesser(epoch),
			witnessed_until_sender,
		};

		Ok(WitnesserInitResult::Created((witnesser, Box::pin(block_stream))))
	}

}
