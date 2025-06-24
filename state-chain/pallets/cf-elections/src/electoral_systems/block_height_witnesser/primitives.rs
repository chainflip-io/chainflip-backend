use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use generic_typeinfo_derive::GenericTypeInfo;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::vec_deque::VecDeque;

use crate::electoral_systems::state_machine::core::{def_derive, defx};

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
		pub first: Header<T>,
		pub(crate) headers: VecDeque<Header<T>>,
	}
	validate this (else NonemptyContinuousHeadersError) {
		is_nonempty: this.headers.len() > 0,
		matching_hashes: pairs.clone().all(|(a, b)| a.hash == b.parent_hash),
		continuous_heights: pairs.clone().all(|(a, b)| a.block_height.saturating_forward(1) == b.block_height),

		( where pairs = this.headers.iter().zip(this.headers.iter().skip(1)) )
	}
}
// impl<T: ChainTypes, X: IntoIterator<Item = Header<T>>> From<X> for NonemptyContinuousHeaders<T> {
// 	fn from(value: X) -> Self {
// 		NonemptyContinuousHeaders { 
// 			first: value.into_iter().next().unwr,
// 			headers: value.into_iter().collect() 
// 		}
// 	}
// }
impl<T: ChainTypes> NonemptyContinuousHeaders<T> {
	pub fn try_new(
		mut headers: VecDeque<Header<T>>,
	) -> Result<Self, NonemptyContinuousHeadersError<T>> {
		if let Some(header) = headers.pop_front() {
			Ok(Self {
				first: header,
				headers
			})
		} else {
			Err(NonemptyContinuousHeadersError::<T>::is_nonempty)
		}
	}
	pub fn first_height(&self) -> T::ChainBlockNumber {
		self.first.block_height
		// self.headers.front().map(|h| h.block_height)
	}
	pub fn last(&self) -> &Header<T> {
		self.headers.back().unwrap_or_else(||{
			&self.first
		})
	}
	pub fn first(&self) -> &Header<T> {
		&self.first
		// self.headers.front().unwrap()
	}
	pub fn contains(&self, block_height: &T::ChainBlockNumber) -> bool {
		self.first_height() <= *block_height && *block_height <= self.last().block_height
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
		if self.last().block_height.saturating_forward(1) == other.first().block_height {
			if self.last().hash == other.first().parent_hash {
				self.headers.append(&mut other.headers.clone());
				Ok(MergeInfo { removed: VecDeque::new(), added: other.headers })
			} else {
				Err(MergeFailure::Reorg {
					new_block: other.first().clone(),
					existing_wrong_parent: self.headers.back().cloned(),
				})
			}
		} else if self.first().block_height == other.first().block_height {
			let mut self_headers = self.headers.clone();
			let mut other_headers = other.headers.clone();
			let common_headers = extract_common_prefix(&mut self_headers, &mut other_headers);
			self.headers = common_headers;
			self.headers.append(&mut other_headers.clone());
			Ok(MergeInfo { removed: self_headers, added: other_headers })
		} else {
			Err(MergeFailure::InternalError)
		}
	}
	
	pub fn trim_to_length(&mut self, target_length: usize) {
		if self.headers.len() >= target_length && target_length > 0 {
			let mut first = self.headers.pop_front();
			while self.headers.len() >= target_length {
				first = self.headers.pop_front();
			}
			self.first = first.unwrap();
		} else {
			self.first = self.last().clone();
			self.headers = VecDeque::new();
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

/// Information returned if the `merge` function for `NonEmptyContinuousHeaders` encountered an
/// error.
#[derive(Debug)]
pub enum MergeFailure<T: ChainTypes> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	Reorg { new_block: Header<T>, existing_wrong_parent: Option<Header<T>> },

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

		const SAFETY_BUFFER: usize = 3;
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
						first_chain_clone.first_height().unwrap()
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
			let final_chain = first_chain.clone().merge(second_chain.clone());
			match final_chain {
				Ok(merge_result) => {
					let mut first_headers = first_chain.headers;
					let mut second_headers = second_chain.headers;

					extract_common_prefix(&mut first_headers, &mut second_headers);
					prop_assert_eq!(merge_result.added, second_headers, "Added blocks do not match");
				},
				Err(merge_failure) => {
					match merge_failure {
							MergeFailure::Reorg { new_block, existing_wrong_parent } => {
								prop_assert_eq!(second_chain.first(), &new_block, "New blocks do not match");
								prop_assert_eq!(Some(first_chain.last()), existing_wrong_parent.as_ref(), "Existing wrong parent do not match");
							},
							MergeFailure::InternalError => {
								prop_assert!((first_chain.last().block_height != second_chain.first_height().unwrap()) || (first_chain.first_height() != second_chain.first_height()));
							},
						}
				},
			}
		}
	}
}
