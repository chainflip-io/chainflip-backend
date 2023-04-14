use async_trait::async_trait;
use cf_primitives::EpochIndex;
use tokio::sync::oneshot;

use crate::multisig::HasChainTag;

use super::{
	checkpointing::WitnessedUntil,
	epoch_witnesser::{self, EpochWitnesser},
	ChainBlockNumber, HasBlockNumber,
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
		epoch_witnesser::run_witnesser_block_stream(
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
