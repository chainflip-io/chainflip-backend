use core::{
	iter::Step,
	ops::{Add, AddAssign, Range, RangeInclusive, Rem, Sub, SubAssign},
};

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	electoral_systems::block_height_tracking::RangeOfBlockWitnessRanges,
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_chains::{btc::BlockNumber, witness_period::BlockWitnessRange};
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode};
use frame_support::{
	ensure,
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::traits::{AtLeast32BitUnsigned, One, Saturating},
	Parameter,
};
use itertools::Itertools;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};

use super::{state_machine::Validate, BlockHeightTrait, ChainProgress};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct Header<BlockHash, BlockNumber> {
	pub block_height: BlockNumber,
	pub hash: BlockHash,
	pub parent_hash: BlockHash,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct MergeInfo<H, N> {
	pub removed: VecDeque<Header<H, N>>,
	pub added: VecDeque<Header<H, N>>,
}

impl<H, N: Copy + Step> MergeInfo<H, N> {
	pub fn into_chain_progress(&self) -> Option<ChainProgress<N>> {
		if let (Some(first_added), Some(last_added)) = (self.added.front(), self.added.back()) {
			if let (Some(first_removed), Some(last_removed)) =
				(self.removed.front(), self.removed.back())
			{
				Some(ChainProgress::Reorg(first_added.block_height..=last_added.block_height))
			} else {
				Some(ChainProgress::Continuous(first_added.block_height..=last_added.block_height))
			}
		} else {
			None
		}
	}

	pub fn get_added_block_heights(&self) -> Option<RangeInclusive<N>> {
		if let (Some(first), Some(last)) = (self.added.front(), self.added.back()) {
			Some(first.block_height..=last.block_height)
		} else {
			None
		}
	}
}

pub enum MergeFailure<H, N> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	ReorgWithUnknownRoot { new_block: Header<H, N>, existing_wrong_parent: Option<Header<H, N>> },

	// /// This means that we have requested blocks which start higher than our last highest
	// block, /// should not happen if everything goes well.
	// MissingBlocks { range: Range<N>},
	InternalError(&'static str),
}

pub fn extract_common_prefix<A: Eq>(a: &mut VecDeque<A>, b: &mut VecDeque<A>) -> VecDeque<A> {
	let mut prefix = VecDeque::new();

	while a.front().is_some() && (a.front() == b.front()) {
		prefix.push_back(a.pop_front().unwrap());
		b.pop_front();
	}

	prefix
}

pub fn trim_to_length<A>(items: &mut VecDeque<A>, target_length: usize) -> VecDeque<A> {
	let mut result = VecDeque::new();
	while items.len() > target_length {
		if let Some(front) = items.pop_front() {
			result.push_back(front);
		}
	}
	result
}

pub fn head_and_tail<A: Clone>(mut items: &VecDeque<A>) -> Option<(A, VecDeque<A>)> {
	let items = items.clone();
	items.clone().pop_front().map(|head| (head, items))
}

#[derive(Debug)]
pub enum VoteValidationError {
	BlockHeightsNotContinuous,
	ParentHashMismatch,
	EmptyVote,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
/// Invariant:
/// This should always be a continuous chain of block headers
pub struct ChainBlocks<H, N> {
	pub headers: VecDeque<Header<H, N>>,
}

impl<H, N: Copy> ChainBlocks<H, N> {
	pub fn current_state_as_no_chain_progress(&self) -> ChainProgress<N> {
		if let Some(last) = self.headers.back() {
			ChainProgress::None(last.block_height)
		} else {
			ChainProgress::WaitingForFirstConsensus
		}
	}

	pub fn first_height(&self) -> Option<N> {
		self.headers.front().map(|h| h.block_height)
	}
}

impl<H, N> Validate for ChainBlocks<H, N>
where
	H: PartialEq + Clone,
	N: PartialEq + Ord + Copy + BlockHeightTrait,
{
	type Error = VoteValidationError;

	fn is_valid(&self) -> Result<(), Self::Error> {
		let mut pairs = self.headers.iter().zip(self.headers.iter().skip(1));

		if !pairs.clone().all(|(a, b)| N::forward(a.block_height, 1) == b.block_height) {
			Err(VoteValidationError::BlockHeightsNotContinuous)
		} else if !pairs.all(|(a, b)| a.hash == b.parent_hash) {
			Err(VoteValidationError::ParentHashMismatch)
		} else {
			Ok(())
		}
	}
}

pub fn validate_vote_and_height<H: PartialEq + Clone, N: PartialEq>(
	next_height: N,
	other: &VecDeque<Header<H, N>>,
) -> Result<(), VoteValidationError>
where
	N: Ord + Copy + BlockHeightTrait,
{
	// a vote has to be nonempty
	if other.len() == 0 {
		return Err(VoteValidationError::EmptyVote)
	}

	// a vote has to start with the next block we expect
	if other.front().unwrap().block_height != next_height {
		return Err(VoteValidationError::BlockHeightsNotContinuous)
	}

	// a vote has to be continous
	ChainBlocks { headers: other.clone() }.is_valid() // validate_continous_headers(other)
}

pub enum ChainBlocksMergeResult<N> {
	Extended { new_highest: N },
	FailedMissing { range: Range<N> },
}

impl<H: Eq + Clone, N: Ord + Copy + Step> ChainBlocks<H, N> {
	//
	// We have the following assumptions:
	//
	// Assumptions:
	//   1. `other`: a. is well-formed (contains incrementing heights) b. is nonempty
	// 	 2. `self`: a. is well-formed (contains incrementing heights)
	//   3. one of the following cases holds
	//       - case 1: `other` starts exactly after `self` ends OR self is `Default::default()`
	//       - case 2: (`self` and `other` start at the same block) AND (self is nonempty)
	//
	pub fn merge(
		&mut self,
		other: VecDeque<Header<H, N>>,
	) -> Result<MergeInfo<H, N>, MergeFailure<H, N>> {
		// assumption (1b)
		let other_head = other
			.front()
			.ok_or(MergeFailure::InternalError("expected other to not be empty!".into()))?;

		let self_next_height = N::forward(self.headers.back().unwrap().block_height, 1);

		if self_next_height == other_head.block_height {
			// this is "assumption (3): case 1"
			//
			// This means that our new blocks start exactly after the ones we already have,
			// so we have to append them to our existing ones. And make sure that the hash/parent
			// hashes match.

			if match self.headers.back() {
				None => true,
				Some(h) => other_head.parent_hash == h.hash,
			} {
				// self.next_height = other.back().unwrap().block_height + 1u32.into();
				self.headers.append(&mut other.clone());
				Ok(MergeInfo { removed: VecDeque::new(), added: other })
			} else {
				Err(MergeFailure::ReorgWithUnknownRoot {
					new_block: other_head.clone(),
					existing_wrong_parent: self.headers.back().cloned(),
				})
			}
		} else {
			// this is "assumption (3): case 2"

			let self_head = self
				.headers
				.front()
				.ok_or(MergeFailure::InternalError(
					"case 2: expected self to not be empty!".into(),
				))?
				.clone();

			if self_head.block_height == other_head.block_height {
				// extract common prefix of headers
				let mut self_headers = self.headers.clone();
				let mut other_headers = other.clone();
				let common_headers = extract_common_prefix(&mut self_headers, &mut other_headers);

				// set headers to `common_headers` + `other_headers`
				self.headers = common_headers;
				self.headers.append(&mut other_headers.clone());
				// self.next_height = self_head.block_height + (self.headers.len() as u32).into();

				Ok(MergeInfo { removed: self_headers, added: other_headers })
			} else {
				Err(MergeFailure::InternalError("expected either case 1 or case 2 to hold!".into()))
			}
		}
	}
}