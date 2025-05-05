use core::ops::{Range, RangeInclusive};

use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::vec_deque::VecDeque;

use super::{super::state_machine::core::Validate, ChainProgress, ChainTypes};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct Header<T: ChainTypes> {
	pub block_height: T::ChainBlockNumber,
	pub hash: T::ChainBlockHash,
	pub parent_hash: T::ChainBlockHash,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct MergeInfo<T: ChainTypes> {
	pub removed: VecDeque<Header<T>>,
	pub added: VecDeque<Header<T>>,
}

impl<T: ChainTypes> MergeInfo<T> {
	pub fn into_chain_progress(&self) -> Option<ChainProgress<T>> {
		if let (Some(first_added), Some(last_added)) = (self.added.front(), self.added.back()) {
			let hashes = self
				.added
				.iter()
				.map(|header| (header.block_height, header.hash.clone()))
				.collect();
			Some(ChainProgress::Range(hashes, first_added.block_height..=last_added.block_height))
		} else {
			None
		}
	}

	pub fn get_added_block_heights(&self) -> Option<RangeInclusive<T::ChainBlockNumber>> {
		if let (Some(first), Some(last)) = (self.added.front(), self.added.back()) {
			Some(first.block_height..=last.block_height)
		} else {
			None
		}
	}
}

pub enum MergeFailure<T: ChainTypes> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	ReorgWithUnknownRoot { new_block: Header<T>, existing_wrong_parent: Option<Header<T>> },

	// /// This means that we have requested blocks which start higher than our last highest
	// block, /// should not happen if everything goes well.
	// MissingBlocks { range: Range<N>},
	InternalError(&'static str),
}

pub fn extract_common_prefix<A: PartialEq>(
	a: &mut VecDeque<A>,
	b: &mut VecDeque<A>,
) -> VecDeque<A> {
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

pub fn head_and_tail<A: Clone>(items: &VecDeque<A>) -> Option<(A, VecDeque<A>)> {
	let items = items.clone();
	items.clone().pop_front().map(|head| (head, items))
}

#[derive(Debug)]
pub enum VoteValidationError {
	BlockHeightsNotContinuous,
	ParentHashMismatch,
	EmptyVote,
	BlockNotMatchingRequestedHeight,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
/// Invariant:
/// This should always be a continuous chain of block headers
pub struct ChainBlocks<T: ChainTypes> {
	pub headers: VecDeque<Header<T>>,
}

impl<T: ChainTypes> ChainBlocks<T> {
	pub fn first_height(&self) -> Option<T::ChainBlockNumber> {
		self.headers.front().map(|h| h.block_height)
	}
}

impl<T: ChainTypes> Validate for ChainBlocks<T> {
	type Error = VoteValidationError;

	fn is_valid(&self) -> Result<(), Self::Error> {
		let mut pairs = self.headers.iter().zip(self.headers.iter().skip(1));

		if !pairs
			.clone()
			.all(|(a, b)| a.block_height.saturating_forward(1) == b.block_height)
		{
			Err(VoteValidationError::BlockHeightsNotContinuous)
		} else if !pairs.all(|(a, b)| a.hash == b.parent_hash) {
			Err(VoteValidationError::ParentHashMismatch)
		} else {
			Ok(())
		}
	}
}

pub fn validate_vote_and_height<T: ChainTypes>(
	next_height: T::ChainBlockNumber,
	other: &VecDeque<Header<T>>,
) -> Result<(), VoteValidationError> {
	// a vote has to be nonempty
	if other.is_empty() {
		return Err(VoteValidationError::EmptyVote)
	}

	// a vote has to start with the next block we expect
	if other.front().unwrap().block_height != next_height {
		return Err(VoteValidationError::BlockHeightsNotContinuous)
	}

	// a vote has to be continous
	ChainBlocks::<T> { headers: other.clone() }.is_valid() // validate_continous_headers(other)
}

pub enum ChainBlocksMergeResult<N> {
	Extended { new_highest: N },
	FailedMissing { range: Range<N> },
}

impl<T: ChainTypes> ChainBlocks<T> {
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
	pub fn merge(&mut self, other: VecDeque<Header<T>>) -> Result<MergeInfo<T>, MergeFailure<T>> {
		// assumption (1b)
		let other_head = other
			.front()
			.ok_or(MergeFailure::InternalError("expected other to not be empty!"))?;

		let self_next_height = self.headers.back().unwrap().block_height.saturating_forward(1);

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
				.ok_or(MergeFailure::InternalError("case 2: expected self to not be empty!"))?
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
				Err(MergeFailure::InternalError("expected either case 1 or case 2 to hold!"))
			}
		}
	}
}
