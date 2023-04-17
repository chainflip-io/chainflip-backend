use std::sync::Arc;

use async_trait::async_trait;
use cf_primitives::EpochIndex;
use tokio::sync::oneshot;

use crate::multisig::{HasChainTag, PersistentKeyDB};

use super::{
	checkpointing::{
		get_witnesser_start_block_with_checkpointing, StartCheckpointing, WitnessedUntil,
	},
	epoch_process_runner::{self, EpochProcessGenerator, EpochWitnesser, WitnesserInitResult},
	ChainBlockNumber, EpochStart, HasBlockNumber,
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

	async fn do_witness(
		&mut self,
		block: W::Block,
		state: &mut Self::StaticState,
	) -> anyhow::Result<()> {
		let block_number = block.block_number();

		self.witnesser.process_block(block, state).await?;

		self.witnessed_until_sender
			.send(WitnessedUntil {
				epoch_index: self.epoch_index,
				block_number: block_number.into(),
			})
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
		let (from_block, witnessed_until_sender) =
			match get_witnesser_start_block_with_checkpointing::<
				<Generator::Witnesser as BlockWitnesser>::Chain,
			>(epoch.epoch_index, epoch.block_number, self.db.clone())
			.await
			// TODO: print chain name
			.expect("Failed to start witnesser checkpointing")
			{
				StartCheckpointing::Started((from_block, witnessed_until_sender)) =>
					(from_block, witnessed_until_sender),
				StartCheckpointing::AlreadyWitnessedEpoch => return Ok(WitnesserInitResult::EpochSkipped),
			};

		let block_stream = self.generator.get_block_stream(from_block).await?;

		let witnesser = BlockWitnesserWrapper {
			epoch_index: epoch.epoch_index,
			witnesser: self.generator.create_witnesser(epoch),
			witnessed_until_sender,
		};

		Ok(WitnesserInitResult::Created((witnesser, block_stream)))

	}

}
