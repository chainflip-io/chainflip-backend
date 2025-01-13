use core::{
	iter::Step,
	ops::{RangeInclusive, Rem, Sub},
};

use super::state_machine::{
	core::{Indexed, Validate},
	state_machine::StateMachine,
	state_machine_es::SMInput,
};
use crate::{electoral_systems::state_machine::core::SaturatingStep, CorruptStorageError};
use cf_chains::witness_period::{BlockWitnessRange, BlockZero};
use codec::{Decode, Encode};
use frame_support::{
	ensure,
	pallet_prelude::MaxEncodedLen,
	sp_runtime::traits::{Block, One, Saturating},
};
use primitives::{trim_to_length, ChainBlocks, Header, MergeFailure, VoteValidationError};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

#[cfg(test)]
use proptest_derive::Arbitrary;

pub mod primitives;
pub mod consensus;
pub mod state_machine;

pub trait BlockHeightTrackingTypes: Ord + PartialEq + Clone + sp_std::fmt::Debug + 'static {
	const SAFETY_MARGIN: usize;
	type ChainBlockNumber: Serialize
		+ for<'a> Deserialize<'a>
		+ PartialEq
		+ Ord
		+ Copy
		+ Step
		+ BlockZero
		+ sp_std::fmt::Debug
		+ 'static;
	type ChainBlockHash: Serialize
		+ for<'a> Deserialize<'a>
		+ PartialEq
		+ Eq
		+ Ord
		+ Clone
		+ sp_std::fmt::Debug
		+ 'static;
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BlockHeightTrackingProperties<BlockNumber> {
	/// An election starts with a given block number,
	/// meaning that engines have to submit all blocks they know of starting with this height.
	pub witness_from_index: BlockNumber,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct RangeOfBlockWitnessRanges<ChainBlockNumber> {
	pub witness_from_root: ChainBlockNumber,
	pub witness_to_root: ChainBlockNumber,
	pub witness_period: ChainBlockNumber,
}

impl<
		ChainBlockNumber: Saturating
			+ One
			+ Copy
			+ PartialOrd
			+ Step
			+ Into<u64>
			+ Sub<ChainBlockNumber, Output = ChainBlockNumber>
			+ Rem<ChainBlockNumber, Output = ChainBlockNumber>
			+ Saturating
			+ Eq,
	> RangeOfBlockWitnessRanges<ChainBlockNumber>
{
	pub fn try_new(
		witness_from_root: ChainBlockNumber,
		witness_to_root: ChainBlockNumber,
		witness_period: ChainBlockNumber,
	) -> Result<Self, CorruptStorageError> {
		ensure!(witness_from_root <= witness_to_root, CorruptStorageError::new());

		Ok(Self { witness_from_root, witness_to_root, witness_period })
	}

	pub fn block_witness_ranges(&self) -> Result<Vec<BlockWitnessRange<ChainBlockNumber>>, ()> {
		(self.witness_from_root..=self.witness_to_root)
			.step_by(Into::<u64>::into(self.witness_period) as usize)
			.map(|root| BlockWitnessRange::try_new(root, self.witness_period))
			.collect::<Result<Vec<_>, _>>()
	}

	pub fn witness_to_root(&self) -> ChainBlockNumber {
		self.witness_to_root
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum OldChainProgress<ChainBlockNumber> {
	// Block witnesser will discard any elections that were started for this range and start them
	// again since we've detected a reorg
	Reorg(RangeOfBlockWitnessRanges<ChainBlockNumber>),
	// the chain is just progressing as a normal chain of hashes
	Continuous(RangeOfBlockWitnessRanges<ChainBlockNumber>),
	// there was no update to the witnessed block headers
	None(ChainBlockNumber),
	// We are starting up and don't have consensus on a block number yet
	WaitingForFirstConsensus,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgress<ChainBlockNumber> {
	// Range of new block heights witnessed. If this is not consecutive, it means that 
	Range(RangeInclusive<ChainBlockNumber>),
	// there was no update to the witnessed block headers
	None,
}

impl<N: Ord> Validate for ChainProgress<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		use ChainProgress::*;
		match self {
			Range(range) => {
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


