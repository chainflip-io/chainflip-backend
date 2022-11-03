//! Common Witnesser functionality

use cf_primitives::EpochIndex;

pub mod block_head_stream_from;
pub mod epoch_witnesser;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct EpochStart {
	pub epoch_index: EpochIndex,
	pub eth_block: <cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber,
	pub current: bool,
	pub participant: bool,
}

pub trait BlockNumberable {
	fn block_number(&self) -> u64;
}
