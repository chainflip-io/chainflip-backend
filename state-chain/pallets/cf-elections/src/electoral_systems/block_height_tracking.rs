use core::{
	iter::Step,
	ops::{Add, AddAssign, Range, RangeInclusive, Rem, Sub, SubAssign},
};

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_chains::{
	assets::arb::Chain,
	btc::BlockNumber,
	witness_period::{BlockWitnessRange, BlockZero},
};
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode};
use consensus::{ConsensusMechanism, StagedConsensus, SupermajorityConsensus, Threshold};
use frame_support::{
	ensure,
	pallet_prelude::{MaxEncodedLen, MaybeSerializeDeserialize, Member},
	sp_runtime::traits::{AtLeast32BitUnsigned, One, Saturating},
	Parameter,
};
use itertools::Itertools;
use primitives::{
	trim_to_length, validate_vote_and_height, ChainBlocks, Header, MergeFailure,
	VoteValidationError,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};
use state_machine::{Indexed, StateMachine, Validate};

#[cfg(test)]
use proptest_derive::Arbitrary;

pub mod consensus;
pub mod primitives;
pub mod state_machine;
pub mod state_machine_es;

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
	// Block witnesser will discard any elections that were started for this range and start them
	// again since we've detected a reorg
	Reorg(RangeInclusive<ChainBlockNumber>),
	// the chain is just progressing as a normal chain of hashes
	Continuous(RangeInclusive<ChainBlockNumber>),
	// there was no update to the witnessed block headers
	None(ChainBlockNumber),
	// We are starting up and don't have consensus on a block number yet
	WaitingForFirstConsensus,
}

//-------- implementation of block height tracking as a state machine --------------

trait BlockHeightTrait = PartialEq + Ord + Copy + Step + BlockZero;

pub struct BlockHeightTrackingConsensus<ChainBlockNumber, ChainBlockHash> {
	votes: Vec<InputHeaders<ChainBlockHash, ChainBlockNumber>>,
}

impl<ChainBlockNumber, ChainBlockHash> Default
	for BlockHeightTrackingConsensus<ChainBlockNumber, ChainBlockHash>
{
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<
		ChainBlockNumber: BlockHeightTrait + sp_std::fmt::Debug,
		ChainBlockHash: Clone + PartialEq + Ord + sp_std::fmt::Debug,
	> ConsensusMechanism for BlockHeightTrackingConsensus<ChainBlockNumber, ChainBlockHash>
{
	type Vote = InputHeaders<ChainBlockHash, ChainBlockNumber>;
	type Result = InputHeaders<ChainBlockHash, ChainBlockNumber>;
	type Settings = (Threshold, BlockHeightTrackingProperties<ChainBlockNumber>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		// let num_authorities = consensus_votes.num_authorities();

		let (threshold, properties) = settings;

		if properties.witness_from_index.is_zero() {
			// This is the case for finding an appropriate block number to start witnessing from

			let mut consensus: SupermajorityConsensus<_> = SupermajorityConsensus::default();

			for vote in &self.votes {
				// we currently only count votes consisting of a single block height
				// there has to be a supermajority voting for the exact same header
				if vote.0.len() == 1 {
					consensus.insert_vote(vote.0[0].clone())
				}
			}

			consensus
				.check_consensus(&threshold)
				.map(|result| {
					let mut headers = VecDeque::new();
					headers.push_back(result);
					InputHeaders(headers)
				})
				.map(|result| {
					log::info!("block_height: initial consensus: {result:?}");
					result
				})
		} else {
			// This is the actual consensus finding, once the engine is running

			let mut consensus: StagedConsensus<SupermajorityConsensus<Self::Vote>, usize> =
				StagedConsensus::new();

			for mut vote in self.votes.clone() {
				// ensure that the vote is valid
				if let Err(err) = validate_vote_and_height(properties.witness_from_index, &vote.0) {
					log::warn!("received invalid vote: {err:?} ");
					continue;
				}

				// we count a given vote as multiple votes for all nonempty subchains
				while vote.0.len() > 0 {
					consensus.insert_vote((vote.0.len(), vote.clone()));
					vote.0.pop_back();
				}
			}

			consensus.check_consensus(&threshold).map(|result| {
				log::info!(
					"(witness_from: {:?}): successful consensus for ranges: {:?}..={:?}",
					properties,
					result.0.front(),
					result.0.back()
				);
				result
			})
		}
	}
}

//------------------------ input headers ---------------------------
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct InputHeaders<H, N>(pub VecDeque<Header<H, N>>);

#[cfg(test)]
mod tests {

	use core::iter::Step;

	use crate::electoral_systems::block_height_tracking::state_machine::{Indexed, Validate};
	use proptest::{
		prelude::{any, prop, Arbitrary, Just, Strategy},
		prop_oneof, proptest,
	};

	use super::{
		primitives::Header, state_machine::StateMachine, BHWState, BlockHeightTrackingDSM,
		InputHeaders,
	};

	pub fn arb_input_headers<H: Arbitrary + Clone, N: Arbitrary + Clone + 'static + Step>(
		witness_from: N,
	) -> impl Strategy<Value = InputHeaders<H, N>> {
		// TODO: handle the case where `witness_from` = 0.

		prop::collection::vec(any::<H>(), 2..10).prop_map(move |data| {
			let headers =
				data.iter().zip(data.iter().skip(1)).enumerate().map(|(ix, (h0, h1))| Header {
					block_height: N::forward(witness_from.clone(), ix),
					hash: h1.clone(),
					parent_hash: h0.clone(),
				});
			InputHeaders::<H, N>(headers.collect())
		})
	}

	pub fn arb_state<H: Arbitrary + Clone, N: Arbitrary + Clone + 'static + Step>(
	) -> impl Strategy<Value = BHWState<H, N>> {
		prop_oneof![
			Just(BHWState::Starting),
			(any::<N>(), any::<bool>()).prop_flat_map(move |(n, is_reorg_without_known_root)| {
				arb_input_headers(n).prop_map(move |headers| {
					let witness_from = if is_reorg_without_known_root {
						headers.0.front().unwrap().block_height.clone()
					} else {
						N::forward(headers.0.back().unwrap().block_height.clone(), 1)
					};
					BHWState::Running { headers: headers.0, witness_from }
				})
			}),
		]
	}

	#[test]
	pub fn test_dsm() {
		BlockHeightTrackingDSM::<6, u32, bool>::test(arb_state(), |index| {
			arb_input_headers(index.witness_from_index).boxed()
		});
	}
}

impl<H, N: BlockZero + Copy + PartialEq> Indexed for InputHeaders<H, N> {
	type Index = BlockHeightTrackingProperties<N>;

	fn has_index(&self, base: &Self::Index) -> bool {
		if base.witness_from_index.is_zero() {
			true
		} else {
			match self.0.front() {
				Some(first) => first.block_height == base.witness_from_index,
				None => false,
			}
		}
	}
}

impl<H: PartialEq + Clone, N: BlockHeightTrait> Validate for InputHeaders<H, N> {
	type Error = VoteValidationError;
	fn is_valid(&self) -> Result<(), Self::Error> {
		if self.0.len() == 0 {
			Err(VoteValidationError::EmptyVote)
		} else {
			ChainBlocks { headers: self.0.clone() }.is_valid()
		}
	}
}

//------------------------ state ---------------------------

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum BHWState<H, N> {
	Starting,
	Running { headers: VecDeque<Header<H, N>>, witness_from: N },
}

impl<H, N> Default for BHWState<H, N> {
	fn default() -> Self {
		Self::Starting
	}
}

impl<H: Clone + PartialEq, N: BlockHeightTrait> Validate for BHWState<H, N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			BHWState::Starting => Ok(()),

			BHWState::Running { headers, witness_from: _ } =>
				if headers.len() > 0 {
					InputHeaders(headers.clone())
						.is_valid()
						.map_err(|_| "blocks should be continuous")
				} else {
					Err("Block height tracking state should always be non-empty after start-up.")
				},
		}
	}
}

//------------------------ output ---------------------------
impl<A, B: sp_std::fmt::Debug + Clone> Validate for Result<A, B> {
	type Error = B;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			Ok(_) => Ok(()),
			Err(err) => Err(err.clone()),
		}
	}
}

//------------------------ state machine ---------------------------
pub struct BlockHeightTrackingDSM<const SAFETY_MARGIN: usize, ChainBlockNumber, ChainBlockHash> {
	_phantom: core::marker::PhantomData<(ChainBlockNumber, ChainBlockHash)>,
}

impl<
		const SAFETY_MARGIN: usize,
		H: PartialEq + Eq + Clone + sp_std::fmt::Debug + 'static,
		N: BlockHeightTrait + sp_std::fmt::Debug + 'static,
	> StateMachine for BlockHeightTrackingDSM<SAFETY_MARGIN, N, H>
{
	type State = BHWState<H, N>;
	type DisplayState = ChainProgress<N>;
	type Input = InputHeaders<H, N>;
	type Output = Result<ChainProgress<N>, &'static str>;

	fn input_index(s: &Self::State) -> <Self::Input as state_machine::Indexed>::Index {
		let witness_from_index = match s {
			BHWState::Starting => N::zero(),
			BHWState::Running { headers: _, witness_from } => witness_from.clone(),
		};
		BlockHeightTrackingProperties { witness_from_index }
	}

	// specification for step function
	#[cfg(test)]
	fn step_specification(before: &Self::State, input: &Self::Input, after: &Self::State) -> bool {
		match after {
			// the starting case should only ever be possible as the `before` state.
			BHWState::Starting => false,

			// otherwise we know that the after state will be running
			BHWState::Running { headers, witness_from } => match before {
				BHWState::Starting => true,
				BHWState::Running {
					headers: before_headers,
					witness_from: before_witness_from,
				} =>
					(*witness_from == before_headers.front().unwrap().block_height) ||
						(*witness_from == N::forward(headers.back().unwrap().block_height, 1)),
			},
		}
	}

	fn step(s: &mut Self::State, new_headers: Self::Input) -> Self::Output {
		match s {
			BHWState::Starting => {
				let first = new_headers.0.front().unwrap().block_height;
				let last = new_headers.0.back().unwrap().block_height;
				*s = BHWState::Running {
					headers: new_headers.0.clone(),
					witness_from: N::forward(last, 1),
				};
				Ok(ChainProgress::Continuous(first..=last))
			},

			BHWState::Running { headers, witness_from } => {
				let mut chainblocks = ChainBlocks { headers: headers.clone() };

				match chainblocks.merge(new_headers.0) {
					Ok(merge_info) => {
						log::info!(
							"added new blocks: {:?}, replacing these blocks: {:?}",
							merge_info.added,
							merge_info.removed
						);

						let safe_headers = trim_to_length(&mut chainblocks.headers, SAFETY_MARGIN);

						let current_state = chainblocks.current_state_as_no_chain_progress();

						*headers = chainblocks.headers;
						*witness_from = N::forward(headers.back().unwrap().block_height, 1);

						// if we merge after a reorg, and the blocks we got are the same
						// as the ones we previously had, then `into_chain_progress` might
						// return `None`. In that case we return our current state.
						Ok(merge_info.into_chain_progress().unwrap_or(current_state))
					},
					Err(MergeFailure::ReorgWithUnknownRoot {
						new_block,
						existing_wrong_parent,
					}) => {
						log::info!("detected a reorg: got block {new_block:?} whose parent hash does not match the parent block we have recorded: {existing_wrong_parent:?}");
						*witness_from = headers.front().unwrap().block_height;
						Ok(chainblocks.current_state_as_no_chain_progress())
					},

					Err(MergeFailure::InternalError(reason)) => {
						log::error!("internal error in block height tracker: {reason}");
						Err("internal error in block height tracker")
					},
				}
			},
		}
	}

	fn get(s: &Self::State) -> Self::DisplayState {
		match s {
			BHWState::Starting => ChainProgress::WaitingForFirstConsensus,
			BHWState::Running { headers, witness_from: _ } =>
				ChainProgress::None(headers.back().unwrap().block_height),
		}
	}
}