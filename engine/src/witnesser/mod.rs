//! Common Witnesser functionality

use cf_primitives::EpochIndex;

pub mod block_head_stream_from;
pub mod checkpointing;
pub mod epoch_witnesser;

pub type ChainBlockNumber<Chain> = <Chain as cf_chains::Chain>::ChainBlockNumber;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct EpochStart<Chain: cf_chains::Chain> {
	pub epoch_index: EpochIndex,
	pub block_number: ChainBlockNumber<Chain>,
	pub current: bool,
	pub participant: bool,
	pub data: Chain::EpochStartData,
}

pub trait BlockNumberable {
	type BlockNumber;

	fn block_number(&self) -> Self::BlockNumber;
}
