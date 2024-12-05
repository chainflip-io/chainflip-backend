use core::ops::{Add, AddAssign, Range, Sub, SubAssign};

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode};
use frame_support::{
	ensure, pallet_prelude::{MaybeSerializeDeserialize, Member}, sp_runtime::traits::AtLeast32BitUnsigned, Parameter
};
use itertools::Itertools;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

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
pub struct BlockHeightTrackingProperties<BlockNumber> {
	/// An election starts with a given block number,
	/// meaning that engines have to submit all blocks they know of starting with this height.
	pub witness_from_index: BlockNumber
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]

/// Invariant:
/// This should always be a continuous chain of block headers
pub struct ChainBlocks<H,N>{
	pub headers: VecDeque<Header<H, N>>,
	pub next_height: N,
}

impl<H,N> ChainBlocks<H,N> {
}

pub enum ChainBlocksMergeResult<N> {
	Extended { new_highest: N },
	FailedMissing { range: Range<N> }
}


// [10] [11] [12] | [13-wrong] [14-wrong]
// => failure
// => we keep [10] [11] [12] as blocks, and we retry with a longer chain
// =>  [10] [11] [12]
//         | [11b] [12b] [13b]
//
// => if the new chain starts with a block number <= the old chain, and they don't match,
//    we simply use the new chain

// we have an extension if 


struct MergeInfo<H,N> {
	removed: VecDeque<Header<H,N>>,
	added: VecDeque<Header<H,N>>
}

// enum MergeSuccess<H,N> {
// 	Extension { keep: ChainBlocks<H,N>, remove: Vec<Header<H,N>>, add: Vec<Header<H,N>> },
// 	Replacement { remove: Vec<Header<H,N>> }

// }

enum MergeFailure<H,N> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	ReorgWithUnknownRoot { new_block: Header<H,N>, existing_wrong_parent: Option<Header<H,N>> },

	// /// This means that we have requested blocks which start higher than our last highest block,
	// /// should not happen if everything goes well.
	// MissingBlocks { range: Range<N>},

	InternalError
}

enum Either<A,B> {
	Left(A),
	Right(B)
}

impl<A,B> Either<A,B> {
	pub fn left(self) -> Option<A> {
		match self {
			Either::Left(a) => Some(a),
			Either::Right(_) => None,
		}
	}

	pub fn right(self) -> Option<B> {
		match self {
			Either::Left(_) => None,
			Either::Right(b) => Some(b),
		}
	}
}

type Headers<H,N> = VecDeque<Header<H,N>>;

struct MatchSplit<Item> {
	prefix: Option<Either<VecDeque<Item>, VecDeque<Item>>>,
	common : VecDeque<(Item,Item)>,
	postfix: Option<Either<VecDeque<Item>, VecDeque<Item>>>,
}

enum MatchSplitError {
	ItemsMissingAfterLeft,
	ItemsMissingAfterRight,
}

pub fn match_split<Item, Height: Ord>(h: impl Fn(Item) -> Height, left: VecDeque<Item>, right: VecDeque<Item>) -> Result<MatchSplit<Item>, MatchSplitError> {
	todo!()
}

pub fn extract_common_prefix<A: Eq>(a: &mut VecDeque<A>, b: &mut VecDeque<A>) -> VecDeque<A> {
	let mut prefix = VecDeque::new();

	while a.front().is_some() && (a.front() == b.front()) {
		prefix.push_front(a.pop_front().unwrap());
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

pub fn head_and_tail<A: Clone>(mut items: &VecDeque<A>) -> Option<(A , VecDeque<A>)> {
	let items = items.clone();
	items.clone().pop_front().map(|head| (head, items))
}

impl<H: Eq + Clone, N: Ord + From<u32> + Add<N, Output=N> + Sub<N, Output=N> + SubAssign<N> + AddAssign<N> + Copy> ChainBlocks<H,N> {


	fn validate(&self) -> Result<(), String> {

		let mut required_block_height = self.next_height - 1u32.into();
		let mut required_hash = None;

		for header in self.headers.iter().rev() {
			ensure!(header.block_height == required_block_height, "unexpected block height");
			ensure!(Some(&header.hash) == required_hash.as_ref().or(Some(&header.hash)), "wrong hash");

			required_block_height -= 1u32.into();
			required_hash = Some(header.parent_hash.clone());
		}

		Ok(())
	}


	// there are two cases where we want to merge:
	//
    //      1. [aaaaaaa] 
    //                [bbbbbbb]
    //      2. [aaaaaaa] 
    //         [bbbbb]
	//
	// Assumptions:
	//   1. `other`:
	//       a. is well-formed (contains incrementing heights)
	//       b. is nonempty
	//	 2. `self`:
	//       a. is well-formed (contains incrementing heights)
	//   3. one of the following cases holds
	//       - case 1: `other` starts exactly after `self` ends
	//       - case 2: (`self` and `other` start at the same block) AND (self is nonempty)
	//
	pub fn merge(&mut self, other: VecDeque<Header<H,N>>) -> Result<MergeInfo<H,N>, MergeFailure<H,N>> {

		// assumption (1b) 
		let other_head = other.front().ok_or(MergeFailure::InternalError)?;

		if self.next_height == other_head.block_height {
			// this is "assumption (3): case 1"
			//
			// This means that our new blocks start exactly after the ones we already have,
			// so we have to append them to our existing ones. And make sure that the hash/parent hashes match.

			if match self.headers.back() {
				None => true,
				Some(h) => other_head.parent_hash == h.hash
			} {
				self.next_height += (other.len() as u32).into();
				self.headers.append(&mut other.clone());
				Ok(MergeInfo { removed: VecDeque::new(), added: other })
			} else {
				Err(MergeFailure::ReorgWithUnknownRoot { new_block: other_head.clone(), existing_wrong_parent: self.headers.back().cloned() })
			}
			
		} else {
			// this is "assumption (3): case 2"

			let self_head = self.headers.front().ok_or(MergeFailure::InternalError)?.clone();

			if self_head.block_height == other_head.block_height {

				// extract common prefix of headers
				let mut self_headers = self.headers.clone();
				let mut other_headers = other.clone();
				let common_headers = extract_common_prefix(&mut self_headers, &mut other_headers);

				// set headers to `common_headers` + `other_headers`
				self.headers = common_headers;
				self.headers.append(&mut other_headers.clone());
				self.next_height = self_head.block_height + (self.headers.len() as u32).into();

				Ok(MergeInfo {
					removed: self_headers,
					added: other_headers,
				})
			} else {
				Err(MergeFailure::InternalError)
			}
		}
	}


}


#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BlockHeightTrackingState<BlockHash, BlockNumber> {
	/// The headers which the nodes have agreed on previously,
	/// starting with `last_safe_index` + 1
	// pub headers: VecDeque<Header<BlockHash, BlockNumber>>,
	pub headers: ChainBlocks<BlockHash, BlockNumber>,

	/// The last index which is past the `SAFETY_MARGIN`. This means
	/// that reorderings concerning this block and lower should be extremely
	/// rare. Does not mean that they don't happen though, so the code has to
	/// take this into account.
	pub last_safe_index: BlockNumber,
}

pub fn validate_vote<ChainBlockHash, ChainBlockNumber>(properties: BlockHeightTrackingProperties<ChainBlockNumber>, vote: Header<ChainBlockHash, ChainBlockNumber>) {

}


pub struct BlockHeightTracking<
	const SAFETY_MARGIN: usize,
	ChainBlockNumber,
	ChainBlockHash,
	Settings,
	ValidatorId,
> {
	_phantom: core::marker::PhantomData<(ChainBlockNumber, ChainBlockHash, Settings, ValidatorId)>,
}

impl<
		const SAFETY_MARGIN: usize,
		ChainBlockNumber: MaybeSerializeDeserialize + Member + Parameter + Ord + Copy + AtLeast32BitUnsigned,
		ChainBlockHash: MaybeSerializeDeserialize + Member + Parameter + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for BlockHeightTracking<SAFETY_MARGIN, ChainBlockNumber, ChainBlockHash, Settings, ValidatorId>
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = BlockHeightTrackingState<ChainBlockHash, ChainBlockNumber>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = BlockHeightTrackingProperties<ChainBlockNumber>;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<Header<ChainBlockHash, ChainBlockNumber>>;
	type Consensus = VecDeque<Header<ChainBlockHash, ChainBlockNumber>>;
	type OnFinalizeContext = ();

	// Latest safe index
	type OnFinalizeReturn = Option<ChainBlockNumber>;

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}


	/// Emits the most recent block that we deem safe. Thus, any downstream system can process any
	/// blocks up to this block safely.
	// How does it start up -> migrates last processed chain tracking? how do we know we want dupe
	// witnesses?
	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let election_access = ElectoralAccess::election_mut(election_identifier);
			if let Some(new_headers) = election_access.check_consensus()?.has_consensus() {
				election_access.delete();


				let (last_safe_index, next_index) = ElectoralAccess::mutate_unsynchronised_state(|unsynchronised_state| {

					let result = match unsynchronised_state.headers.merge(new_headers) {
						Ok(merge_info) => {
							log::info!("added new blocks: {:?}, replacing these blocks: {:?}", merge_info.added, merge_info.removed);

							let safe_headers = trim_to_length(&mut unsynchronised_state.headers.headers, SAFETY_MARGIN);
							unsynchronised_state.last_safe_index += (safe_headers.len() as u32).into();

							log::info!("the latest safe block height is {:?} (advanced by {})", unsynchronised_state.last_safe_index, safe_headers.len());

							// we only return a new safe index if it actually increased
							if safe_headers.len() > 0 {
								Ok(Some(unsynchronised_state.last_safe_index))
							} else {
								Ok(None)
							}
						},
						Err(MergeFailure::ReorgWithUnknownRoot { new_block, existing_wrong_parent }) => {
							log::warn!("detected a reorg: got block {new_block:?} whose parent hash does not match the parent block we have recorded: {existing_wrong_parent:?}");
							Ok(None)
						},
						Err(MergeFailure::InternalError) => {
							Err(CorruptStorageError {})
						}
					}?;

					Ok((result, unsynchronised_state.headers.next_height))
				})?;

				let properties = BlockHeightTrackingProperties {
					witness_from_index: next_index,
				};

				ElectoralAccess::new_election((), properties, ())?;

				Ok(last_safe_index)

			} else {
				Ok(None)
			}
		} else {
			// If we have no elections to process we should start one to get an updated header.
			// ElectoralAccess::new_election((), (), ())?;
			Ok(None)
		}

		// if we have consensus on a block header, then header - safety is safe.
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		todo!()
		/*
		let num_authorities = consensus_votes.num_authorities();
		let success_threshold = success_threshold_from_share_count(num_authorities);
		let mut active_votes = consensus_votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		Ok(if num_active_votes >= success_threshold {
			// Calculating the median this way means atleast 2/3 of validators would be needed to
			// increase the calculated median.
			let (_, median_vote, _) =
				active_votes.select_nth_unstable((num_authorities - success_threshold) as usize);
			Some(median_vote.clone())
		} else {
			None
		})
 		*/
	}
}
