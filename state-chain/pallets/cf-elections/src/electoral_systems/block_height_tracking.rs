use core::{
	iter::Step,
	ops::{RangeInclusive, Rem, Sub},
};

use super::state_machine::{
	core::{Hook, Indexed, Validate},
	state_machine::StateMachine,
	state_machine_es::SMInput,
};
use crate::CorruptStorageError;
use cf_chains::witness_period::{BlockWitnessRange, BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use frame_support::{
	ensure,
	pallet_prelude::MaxEncodedLen,
	sp_runtime::traits::{Block, One, Saturating},
};
use primitives::{trim_to_length, ChainBlocks, Header, MergeFailure, VoteValidationError};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::vec_deque::VecDeque, fmt::Debug, vec::Vec};

#[cfg(test)]
use proptest_derive::Arbitrary;

pub mod consensus;
pub mod primitives;
pub mod state_machine;

pub trait BlockHeightTrackingTypes: Ord + PartialEq + Clone + Debug + 'static {
	const BLOCK_BUFFER_SIZE: usize;
	type ChainBlockNumber: SaturatingStep
		+ BlockZero
		+ Debug
		+ Copy
		+ Eq
		+ Ord
		+ Serialize
		+ for<'a> Deserialize<'a>
		+ 'static;
	type ChainBlockHash: Serialize
		+ for<'a> Deserialize<'a>
		+ PartialEq
		+ Eq
		+ Ord
		+ Clone
		+ Debug
		+ 'static;

	type BlockHeightChangeHook: Hook<Self::ChainBlockNumber, ()>;
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Deserialize,
	Serialize,
	Ord,
	PartialOrd,
)]
pub struct BlockHeightTrackingProperties<BlockNumber> {
	/// An election starts with a given block number,
	/// meaning that engines have to submit all blocks they know of starting with this height.
	pub witness_from_index: BlockNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgress<ChainBlockNumber> {
	// Range of new block heights witnessed. If this is not consecutive, it means that
	Range(RangeInclusive<ChainBlockNumber>),
	// Range of new block heights, only emitted when there is a consensus for the first time after
	// being started.
	FirstConsensus(RangeInclusive<ChainBlockNumber>),
	// there was no update to the witnessed block headers
	None,
	Progress(ChainBlockNumber),
	Reorg(RangeInclusive<ChainBlockNumber>),
}

impl<N: Ord> Validate for ChainProgress<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		use ChainProgress::*;
		match self {
			Range(range) | FirstConsensus(range) => {
				ensure!(
					range.start() <= range.end(),
					"range a..=b in ChainProgress should have a <= b"
				);
				Ok(())
			},
			None => Ok(()),
		}
	}
}

//-------- implementation of block height tracking as a state machine --------------

pub trait BlockHeightTrait = PartialEq + Ord + Copy + Step + BlockZero;
