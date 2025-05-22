use core::ops::{Range, RangeInclusive};

use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::vec_deque::VecDeque;

use crate::electoral_systems::state_machine::core::defx;

use super::{
	super::state_machine::core::Validate, ChainProgress, ChainProgressFor, ChainTypes, HWTypes,
};

//------------------------ inputs ---------------------------

defx! {
	#[derive(Ord, PartialOrd)]
	pub struct NonemptyContinuousHeaders[T: ChainTypes] {
		pub headers: VecDeque<Header<T>>,
	}
	validate this (else NonemptyContinuousHeadersError) {
		is_nonempty: this.headers.len() > 0,
		matching_hashes: pairs.clone().all(|(a, b)| a.hash == b.parent_hash),
		continuous_heights: pairs.clone().all(|(a, b)| a.block_height.saturating_forward(1) == b.block_height),

		( where pairs = this.headers.iter().zip(this.headers.iter().skip(1)) )
	}
}
impl<T: ChainTypes, X: IntoIterator<Item = Header<T>>> From<X> for NonemptyContinuousHeaders<T> {
	fn from(value: X) -> Self {
		NonemptyContinuousHeaders { headers: value.into_iter().collect() }
	}
}
impl<T: ChainTypes> NonemptyContinuousHeaders<T> {
	pub fn first_height(&self) -> Option<T::ChainBlockNumber> {
		self.headers.front().map(|h| h.block_height)
	}
	pub fn last(&self) -> &Header<T> {
		self.headers.back().unwrap()
	}
	pub fn first(&self) -> &Header<T> {
		self.headers.front().unwrap()
	}
	pub fn merge(
		&mut self,
		other: NonemptyContinuousHeaders<T>,
	) -> Result<MergeInfo<T>, MergeFailure<T>> {
		if self.last().block_height.saturating_forward(1) == other.first().block_height {
			if self.last().hash == other.first().parent_hash {
				self.headers.append(&mut other.headers.clone());
				Ok(MergeInfo { removed: VecDeque::new(), added: other.headers })
			} else {
				Err(MergeFailure::ReorgWithUnknownRoot {
					new_block: other.first().clone(),
					existing_wrong_parent: self.headers.back().cloned(),
				})
			}
		} else {
			if self.first().block_height == other.first().block_height {
				let mut self_headers = self.headers.clone();
				let mut other_headers = other.headers.clone();
				let common_headers = extract_common_prefix(&mut self_headers, &mut other_headers);
				self.headers = common_headers;
				self.headers.append(&mut other_headers.clone());
				Ok(MergeInfo { removed: self_headers, added: other_headers })
			} else {
				Err(MergeFailure::InternalError("expected either case 1 or case 2 to hold!"))
			}
		}
	}
}

defx! {
	#[derive(Ord, PartialOrd)]
	pub struct Header[T: ChainTypes] {
		pub block_height: T::ChainBlockNumber,
		pub hash: T::ChainBlockHash,
		pub parent_hash: T::ChainBlockHash,
	}

	validate this (else HeaderError) {}
}

pub enum ChainBlocksMergeResult<N> {
	Extended { new_highest: N },
	FailedMissing { range: Range<N> },
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct MergeInfo<T: ChainTypes> {
	pub removed: VecDeque<Header<T>>,
	pub added: VecDeque<Header<T>>,
}

impl<T: ChainTypes> MergeInfo<T> {
	pub fn into_chain_progress(&self) -> Option<ChainProgressFor<T>> {
		if let (Some(first_added), Some(last_added)) = (self.added.front(), self.added.back()) {
			let hashes = self
				.added
				.iter()
				.map(|header| (header.block_height, header.hash.clone()))
				.collect();

			use ChainProgress::*;

			let f = if self.removed.is_empty() { Range } else { Reorg };

			Some(f(hashes, first_added.block_height..=last_added.block_height))
		} else {
			None
		}
	}
}

#[derive(Debug)]
pub enum MergeFailure<T: ChainTypes> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	ReorgWithUnknownRoot { new_block: Header<T>, existing_wrong_parent: Option<Header<T>> },
	InternalError(&'static str),
}

fn extract_common_prefix<A: PartialEq>(a: &mut VecDeque<A>, b: &mut VecDeque<A>) -> VecDeque<A> {
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
