use cf_chains::witness_period::SaturatingStep;
use cf_utilities::macros::*;
use codec::{Decode, Encode};
use core::iter;
use generic_typeinfo_derive::GenericTypeInfo;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::vec_deque::VecDeque;

use crate::electoral_systems::state_machine::core::defx;

use super::{super::state_machine::core::Validate, ChainTypes};

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
	#[derive(GenericTypeInfo)]
	#[expand_name_with(T::NAME)]
	pub struct NonemptyContinuousHeaders[T: ChainTypes] {
		first: Header<T>,
		headers: VecDeque<Header<T>>,
	}
	validate this (else NonemptyContinuousHeadersError) {
		empty: true,
		matching_hashes: pairs.clone().all(|(a, b)| a.hash == b.parent_hash),
		continuous_heights: pairs.clone().all(|(a, b)| a.block_height.saturating_forward(1) == b.block_height),

		( where pairs = this.get_headers().into_iter().zip(this.get_headers().into_iter().skip(1)) )
	}
}
/// This is an unsafe implementation of `into()` which panics if the input iterator is of length 0.
/// Only use for tests!
#[cfg(test)]
impl<T: ChainTypes, X: IntoIterator<Item = Header<T>> + Clone> From<X>
	for NonemptyContinuousHeaders<T>
{
	fn from(value: X) -> Self {
		NonemptyContinuousHeaders {
			first: value.clone().into_iter().next().unwrap(),
			headers: value.into_iter().skip(1).collect(),
		}
	}
}
impl<T: ChainTypes> NonemptyContinuousHeaders<T> {
	pub fn try_new(
		mut headers: VecDeque<Header<T>>,
	) -> Result<Self, NonemptyContinuousHeadersError<T>> {
		if let Some(header) = headers.pop_front() {
			let result = Self { first: header, headers };
			result.is_valid()?;
			Ok(result)
		} else {
			Err(NonemptyContinuousHeadersError::<T>::empty)
		}
	}
	pub fn new(header: Header<T>) -> Self {
		Self { first: header, headers: Default::default() }
	}
	pub fn last(&self) -> &Header<T> {
		self.headers.back().unwrap_or(&self.first)
	}
	pub fn first(&self) -> &Header<T> {
		&self.first
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
	) -> Result<MergeInfo<T>, MergeFailure> {
		if self.last().block_height.saturating_forward(1) == other.first().block_height {
			if self.last().hash == other.first().parent_hash {
				self.headers.append(&mut other.get_headers());
				Ok(MergeInfo { removed: VecDeque::new(), added: other.get_headers() })
			} else {
				Err(MergeFailure::Reorg)
			}
		} else if self.first().block_height == other.first().block_height {
			let mut self_headers = self.get_headers();
			let mut other_headers = other.get_headers();
			let mut common_headers = extract_common_prefix(&mut self_headers, &mut other_headers);

			common_headers.append(&mut other_headers.clone());
			//either common_headers or other_headers contain at least 1 element hence this cannot
			// fail
			*self = Self::try_new(common_headers).unwrap();

			Ok(MergeInfo { removed: self_headers, added: other_headers })
		} else {
			Err(MergeFailure::InternalError)
		}
	}

	pub fn get_headers(&self) -> VecDeque<Header<T>> {
		iter::once(&self.first).chain(&self.headers).cloned().collect()
	}

	pub fn trim_to_length(&mut self, target_length: usize) {
		while self.len() > target_length && target_length > 0 {
			self.safe_pop_front();
		}
	}

	/// Extracts the first element. Returns None if there is 1 element only, as it cannot be
	/// removed.
	fn safe_pop_front(&mut self) -> Option<Header<T>> {
		if let Some(next_header) = self.headers.pop_front() {
			let result = self.first.clone();
			self.first = next_header;
			return Some(result);
		}
		None
	}
	/// Extracts the last element. Returns None if there is 1 element only, as it cannot be removed.
	pub fn safe_pop_back(&mut self) -> Option<Header<T>> {
		if let Some(last) = self.headers.pop_back() {
			return Some(last);
		}
		None
	}
	#[expect(clippy::len_without_is_empty)]
	pub fn len(&self) -> usize {
		self.headers.len() + 1usize
	}
}

derive_common_traits! {
	/// Information returned if the `merge` function for `NonEmptyContinuousHeaders` was successful.
	#[derive(Ord, PartialOrd,)]
	pub struct MergeInfo<T: ChainTypes> {
		pub removed: VecDeque<Header<T>>,
		pub added: VecDeque<Header<T>>,
	}
}

/// Information returned if the `merge` function for `NonEmptyContinuousHeaders` encountered an
/// error.
#[derive(Debug)]
pub enum MergeFailure {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	Reorg,

	// Internal error. Should never happen.
	InternalError,
}

defx! {
	/// Block header for a given chain `C` as used by the BHW.
	#[derive(Ord, PartialOrd)]
	#[derive(GenericTypeInfo)]
	#[expand_name_with(C::NAME)]
	pub struct Header[C: ChainTypes] {
		pub block_height: C::ChainBlockNumber,
		pub hash: C::ChainBlockHash,
		pub parent_hash: C::ChainBlockHash,
	}
	validate this (else HeaderError) {}
}

fn extract_common_prefix<A: PartialEq>(a: &mut VecDeque<A>, b: &mut VecDeque<A>) -> VecDeque<A> {
	let mut prefix = VecDeque::new();

	while a.front().is_some() && (a.front() == b.front()) {
		prefix.push_back(a.pop_front().unwrap());
		b.pop_front();
	}
	prefix
}

#[cfg(test)]
mod prop_tests {
	use super::*;
	use proptest::prelude::*;

	#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
	struct MockChainTypes;
	impl ChainTypes for MockChainTypes {
		type ChainBlockNumber = u8;

		type ChainBlockHash = bool;

		const NAME: &'static str = "Mock";
	}

	fn header_strategy() -> impl Strategy<
		Value = (
			NonemptyContinuousHeaders<MockChainTypes>,
			NonemptyContinuousHeaders<MockChainTypes>,
		),
	> {
		NonemptyContinuousHeaders::<MockChainTypes>::arbitrary_with((5, 10)).prop_flat_map(
			|first_chain| {
				any::<bool>().prop_flat_map(move |val| {
					let first_chain_clone = first_chain.clone();
					let len = first_chain_clone.headers.len();
					let start = if val {
						first_chain_clone.first().block_height
					} else {
						first_chain_clone.last().block_height
					};
					NonemptyContinuousHeaders::<MockChainTypes>::arbitrary_with((start, len))
						.prop_map(move |second_chain| (first_chain_clone.clone(), second_chain))
				})
			},
		)
	}

	proptest! {
		#![proptest_config(ProptestConfig {
			cases: 10000, .. ProptestConfig::default()
		  })]
		#[test]
		fn test_headers((first_chain, second_chain) in header_strategy()){
			match first_chain.clone().merge(second_chain.clone()) {
				Ok(merge_result) => {
					let mut first_headers = first_chain.get_headers();
					let mut second_headers = second_chain.get_headers();

					extract_common_prefix(&mut first_headers, &mut second_headers);
					prop_assert_eq!(merge_result.added, second_headers, "Added blocks do not match");
				},
				Err(merge_failure) => {
					match merge_failure {
							MergeFailure::Reorg => {
								prop_assert!(second_chain.first().parent_hash != first_chain.last().hash);
							},
							MergeFailure::InternalError => {
								prop_assert!((first_chain.last().block_height != second_chain.first().block_height) || (first_chain.first().block_height != second_chain.first().block_height));
							},
						}
				},
			}
		}
	}
}
