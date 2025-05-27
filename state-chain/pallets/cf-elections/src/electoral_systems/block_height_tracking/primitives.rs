use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::vec_deque::VecDeque;

use crate::electoral_systems::state_machine::core::{both, def_derive, defx};

use super::{super::state_machine::core::Validate, ChainProgress, ChainTypes};

//------------------------ inputs ---------------------------

defx! {
	/// Non-empty, continous chain of block headers.
	///
	/// This means that:
	///  - There's at least one header
	///  - The `block_height`s of the headers are consequtive
	///  - The `parent_hash` of a header matches with the `hash` of the block before it
	///
	/// These properties are verified as part of the `Validate` implementation derived by the `defx` macro.
	#[derive(Ord, PartialOrd)]
	pub struct NonemptyContinuousHeaders[T: ChainTypes] {
		pub(crate) headers: VecDeque<Header<T>>,
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
	pub fn try_new(
		headers: VecDeque<Header<T>>,
	) -> Result<Self, NonemptyContinuousHeadersError<T>> {
		let result = Self { headers };
		result.is_valid()?;
		Ok(result)
	}
	pub fn first_height(&self) -> Option<T::ChainBlockNumber> {
		self.headers.front().map(|h| h.block_height)
	}
	pub fn last(&self) -> &Header<T> {
		self.headers.back().unwrap()
	}
	pub fn first(&self) -> &Header<T> {
		self.headers.front().unwrap()
	}
	/// Tries to merge the `other` chain of headers into `self`.
	///
	/// This function assumes that either of the following holds:
	///  - `self` and `other` form a continuous chain
	///  - `self` and `other` start at the same block
	///
	/// If this doesn't hold it *will* return a `MergeFailure::InternalError`.
	pub fn merge(
		&mut self,
		other: NonemptyContinuousHeaders<T>,
	) -> Result<MergeInfo<T>, MergeFailure<T>> {
		fn extract_common_prefix<A: PartialEq>(
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
	pub fn trim_to_length(&mut self, target_length: usize) {
		while self.headers.len() > target_length {
			self.headers.pop_front();
		}
	}
}

def_derive! {
	/// Information returned if the `merge` function for `NonEmptyContinuousHeaders` was successful.
	#[derive(Ord, PartialOrd,)]
	pub struct MergeInfo<T: ChainTypes> {
		pub removed: VecDeque<Header<T>>,
		pub added: VecDeque<Header<T>>,
	}
}
impl<T: ChainTypes> MergeInfo<T> {
	pub fn into_chain_progress(&self) -> Option<ChainProgress<T>> {
		if self.added.is_empty() {
			None
		} else {
			Some(ChainProgress {
				headers: self.added.clone().into(),
				removed: both(self.removed.front(), self.removed.back())
					.map(|(first, last)| first.block_height..=last.block_height),
			})
		}
	}
}

/// Information returned if the `merge` function for `NonEmptyContinuousHeaders` encountered an
/// error.
#[derive(Debug)]
pub enum MergeFailure<T: ChainTypes> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	ReorgWithUnknownRoot { new_block: Header<T>, existing_wrong_parent: Option<Header<T>> },
	InternalError(&'static str),
}

defx! {
	/// Block header for a given chain `C` as used by the BHW.
	#[derive(Ord, PartialOrd)]
	pub struct Header[C: ChainTypes] {
		pub block_height: C::ChainBlockNumber,
		pub hash: C::ChainBlockHash,
		pub parent_hash: C::ChainBlockHash,
	}
	validate this (else HeaderError) {}
}
