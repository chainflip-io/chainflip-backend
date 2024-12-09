use core::ops::{Add, AddAssign, Range, RangeInclusive, Sub, SubAssign};

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
	ensure,
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::traits::AtLeast32BitUnsigned,
	Parameter,
};
use itertools::Itertools;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};

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
	pub witness_from_index: BlockNumber,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]

/// Invariant:
/// This should always be a continuous chain of block headers
pub struct ChainBlocks<H, N> {
	pub headers: VecDeque<Header<H, N>>,
	pub next_height: N,
}

impl<H, N> ChainBlocks<H, N> {}

pub enum ChainBlocksMergeResult<N> {
	Extended { new_highest: N },
	FailedMissing { range: Range<N> },
}

pub struct MergeInfo<H, N> {
	removed: VecDeque<Header<H, N>>,
	added: VecDeque<Header<H, N>>,
}

impl<H, N: Copy> MergeInfo<H, N> {
	pub fn get_added_block_heights(&self) -> Option<RangeInclusive<N>> {
		if let (Some(first), Some(last)) = (self.added.front(), self.added.back()) {
			Some(first.block_height..=last.block_height)
		} else {
			None
		}
	}
}

enum MergeFailure<H, N> {
	// If we get a new range of blocks, [lowest_new_block, ...], where the parent of
	// `lowest_new_block` should, by block number, be `existing_wrong_parent`, but who's
	// hash doesn't match with `lowest_new_block`'s parent hash.
	ReorgWithUnknownRoot { new_block: Header<H, N>, existing_wrong_parent: Option<Header<H, N>> },

	// /// This means that we have requested blocks which start higher than our last highest
	// block, /// should not happen if everything goes well.
	// MissingBlocks { range: Range<N>},
	InternalError,
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

pub fn head_and_tail<A: Clone>(mut items: &VecDeque<A>) -> Option<(A, VecDeque<A>)> {
	let items = items.clone();
	items.clone().pop_front().map(|head| (head, items))
}

enum VoteValidationError {
	BlockHeightsNotContinuous,
	ParentHashMismatch,
}

impl<
		H: Eq + Clone,
		N: Ord
			+ From<u32>
			+ Add<N, Output = N>
			+ Sub<N, Output = N>
			+ SubAssign<N>
			+ AddAssign<N>
			+ Copy,
	> ChainBlocks<H, N>
{
	fn validate(&self) -> Result<(), VoteValidationError> {
		let mut required_block_height = self.next_height - 1u32.into();
		let mut required_hash = None;

		for header in self.headers.iter().rev() {
			ensure!(
				header.block_height == required_block_height,
				VoteValidationError::BlockHeightsNotContinuous
			);
			ensure!(
				Some(&header.hash) == required_hash.as_ref().or(Some(&header.hash)),
				VoteValidationError::ParentHashMismatch
			);

			required_block_height -= 1u32.into();
			required_hash = Some(header.parent_hash.clone());
		}

		Ok(())
	}

	//
	// We have the following assumptions:
	//
	// Assumptions:
	//   1. `other`: a. is well-formed (contains incrementing heights) b. is nonempty
	// 	 2. `self`: a. is well-formed (contains incrementing heights)
	//   3. one of the following cases holds
	//       - case 1: `other` starts exactly after `self` ends
	//       - case 2: (`self` and `other` start at the same block) AND (self is nonempty)
	//
	pub fn merge(
		&mut self,
		other: VecDeque<Header<H, N>>,
	) -> Result<MergeInfo<H, N>, MergeFailure<H, N>> {
		// assumption (1b)
		let other_head = other.front().ok_or(MergeFailure::InternalError)?;

		if self.next_height == other_head.block_height {
			// this is "assumption (3): case 1"
			//
			// This means that our new blocks start exactly after the ones we already have,
			// so we have to append them to our existing ones. And make sure that the hash/parent
			// hashes match.

			if match self.headers.back() {
				None => true,
				Some(h) => other_head.parent_hash == h.hash,
			} {
				self.next_height += (other.len() as u32).into();
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

				Ok(MergeInfo { removed: self_headers, added: other_headers })
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

impl<H, N: From<u32>> Default for BlockHeightTrackingState<H, N> {
	fn default() -> Self {
		let headers = ChainBlocks { headers: Default::default(), next_height: 0u32.into() };
		Self { headers, last_safe_index: 0u32.into() }
	}
}

pub fn validate_vote<ChainBlockHash, ChainBlockNumber>(
	properties: BlockHeightTrackingProperties<ChainBlockNumber>,
	vote: Header<ChainBlockHash, ChainBlockNumber>,
) {
}

// -- abstract Consensus for computing solutions
pub trait Consensus: Default {
	type Vote;
	type Result;
	type Settings;
	fn insert_vote(&mut self, vote: Self::Vote);
	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result>;
}

struct SupermajorityConsensus<Vote: PartialEq> {
	votes: BTreeMap<Vote, u32>,
}

struct Threshold {
	threshold: u32,
}

impl<Vote: PartialEq> Default for SupermajorityConsensus<Vote> {
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<Vote: Ord + PartialEq + Clone> Consensus for SupermajorityConsensus<Vote> {
	type Vote = Vote;
	type Result = Vote;
	type Settings = Threshold;

	fn insert_vote(&mut self, vote: Self::Vote) {
		if let Some(count) = self.votes.get_mut(&vote) {
			*count += 1;
		} else {
			self.votes.insert(vote, 1);
		}
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		let best = self.votes.iter().last();

		if let Some((best_vote, best_count)) = best {
			if best_count >= &settings.threshold {
				return Some(best_vote.clone());
			}
		}

		return None;
	}
}

// --
struct StagedConsensus<Stage: Consensus, Index: Ord> {
	stages: BTreeMap<Index, Stage>,
}

impl<Stage: Consensus, Index: Ord> StagedConsensus<Stage, Index> {
	pub fn new() -> Self {
		Self { stages: BTreeMap::new() }
	}
}

impl<Stage: Consensus, Index: Ord> Default for StagedConsensus<Stage, Index> {
	fn default() -> Self {
		Self { stages: Default::default() }
	}
}

impl<Stage: Consensus, Index: Ord + Copy> Consensus for StagedConsensus<Stage, Index> {
	type Result = Stage::Result;
	type Vote = (Index, Stage::Vote);
	type Settings = Stage::Settings;

	fn insert_vote(&mut self, (index, vote): Self::Vote) {
		if let Some(stage) = self.stages.get_mut(&index) {
			stage.insert_vote(vote)
		} else {
			let mut stage = Stage::default();
			stage.insert_vote(vote);
			self.stages.insert(index, stage);
		}
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		// we check all stages starting with the highest index,
		// the first one that has consensus wins
		for (_, stage) in self.stages.iter().rev() {
			if let Some(result) = stage.check_consensus(settings) {
				return Some(result);
			}
		}

		None
	}
}

// -- electoral system
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
	type Vote = vote_storage::bitmap::Bitmap<VecDeque<Header<ChainBlockHash, ChainBlockNumber>>>;
	type Consensus = VecDeque<Header<ChainBlockHash, ChainBlockNumber>>;
	type OnFinalizeContext = ();

	// new block to query for the block witnesser
	type OnFinalizeReturn = Option<RangeInclusive<ChainBlockNumber>>;

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

				let (block_witnesser_range, next_index) =
					ElectoralAccess::mutate_unsynchronised_state(|unsynchronised_state| {
						match unsynchronised_state.headers.merge(new_headers) {
							Ok(merge_info) => {
								log::info!(
									"added new blocks: {:?}, replacing these blocks: {:?}",
									merge_info.added,
									merge_info.removed
								);

								let safe_headers = trim_to_length(
									&mut unsynchronised_state.headers.headers,
									SAFETY_MARGIN,
								);
								unsynchronised_state.last_safe_index +=
									(safe_headers.len() as u32).into();

								log::info!(
									"the latest safe block height is {:?} (advanced by {})",
									unsynchronised_state.last_safe_index,
									safe_headers.len()
								);

								Ok((
									merge_info.get_added_block_heights(),
									unsynchronised_state.headers.next_height,
								))
							},
							Err(MergeFailure::ReorgWithUnknownRoot {
								new_block,
								existing_wrong_parent,
							}) => {
								log::info!("detected a reorg: got block {new_block:?} whose parent hash does not match the parent block we have recorded: {existing_wrong_parent:?}");
								Ok((None, unsynchronised_state.headers.next_height))
							},
							Err(MergeFailure::InternalError) => Err(CorruptStorageError {}),
						}
					})?;

				let properties = BlockHeightTrackingProperties { witness_from_index: next_index };

				log::info!("Starting new election with properties: {:?}", properties);

				ElectoralAccess::new_election((), properties, ())?;

				Ok(block_witnesser_range)
			} else {
				Ok(None)
			}
		} else {
			log::info!("Starting new election with index 0 because no elections exist");

			// If we have no elections to process we should start one to get an updated header.
			// But we have to know which block we want to start witnessing from
			let properties = BlockHeightTrackingProperties {
				// TODO: this block height has to come from storage / governance?
				witness_from_index: 0u32.into(),
			};
			ElectoralAccess::new_election((), properties, ())?;
			Ok(None)
		}
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();

		let mut consensus: StagedConsensus<SupermajorityConsensus<Self::Consensus>, usize> =
			StagedConsensus::new();

		for mut vote in consensus_votes.active_votes() {
			// we count a given vote as multiple votes for all nonempty subchains
			while vote.len() > 0 {
				consensus.insert_vote((vote.len(), vote.clone()));
				vote.pop_back();
			}
		}

		Ok(consensus.check_consensus(&Threshold {
			threshold: success_threshold_from_share_count(num_authorities),
		}))
	}
}
