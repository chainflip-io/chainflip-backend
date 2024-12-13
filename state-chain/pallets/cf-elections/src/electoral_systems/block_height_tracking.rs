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
use cf_chains::{assets::arb::Chain, btc::BlockNumber, witness_period::BlockWitnessRange};
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode};
use consensus::{Consensus, StagedConsensus, SupermajorityConsensus, Threshold};
use frame_support::{
	ensure,
	pallet_prelude::{MaybeSerializeDeserialize, Member, MaxEncodedLen},
	sp_runtime::traits::{AtLeast32BitUnsigned, One, Saturating},
	Parameter,
};
use itertools::Itertools;
use primitives::{trim_to_length, validate_vote_and_height, ChainBlocks, Header, MergeFailure, VoteValidationError};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};
use state_machine::{dependent_state_machine, Fibered, Validate};

pub mod consensus;
pub mod primitives;
pub mod state_machine;
pub mod state_machine_es;

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

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct RangeOfBlockWitnessRanges<ChainBlockNumber> {
	witness_from_root: ChainBlockNumber,
	witness_to_root: ChainBlockNumber,
	witness_period: ChainBlockNumber,
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
pub enum ChainProgress<ChainBlockNumber> {
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
		ChainBlockNumber: MaybeSerializeDeserialize
			+ Member
			+ Parameter
			+ Ord
			+ Copy
			+ AtLeast32BitUnsigned
			+ Into<u64>
			+ Step,
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
	type OnFinalizeReturn = ChainProgress<ChainBlockNumber>;

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	// Emits the range of blocks which should be witnessed next.
	// If a reorg happened then the lower bound of the range is going
	// to be <= a previously emitted (inclusive) upper bound.
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
									merge_info.into_chain_progress().unwrap(),
									unsynchronised_state.headers.next_height,
								))
							},
							Err(MergeFailure::ReorgWithUnknownRoot {
								new_block,
								existing_wrong_parent,
							}) => {
								log::info!("detected a reorg: got block {new_block:?} whose parent hash does not match the parent block we have recorded: {existing_wrong_parent:?}");
								Ok((
									unsynchronised_state
										.headers
										.current_state_as_no_chain_progress(),
									unsynchronised_state
										.headers
										.first_height()
										.unwrap_or(0u32.into()), /* If we have no first height
									                           * recorded, we have to restart
									                           * the election?! */
								))
							},
							Err(MergeFailure::InternalError(reason)) => {
								log::error!("internal error in block height tracker: {reason}");
								Err(CorruptStorageError::new())
							},
						}
					})?;

				let properties = BlockHeightTrackingProperties { witness_from_index: next_index };

				log::info!("Starting new election with properties: {:?}", properties);

				ElectoralAccess::new_election((), properties, ())?;

				Ok(block_witnesser_range)
			} else {
				Ok(ElectoralAccess::unsynchronised_state()?
					.headers
					.current_state_as_no_chain_progress())
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
			Ok(ChainProgress::WaitingForFirstConsensus)
		}
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();

		let properties = election_access.properties()?;
		if properties.witness_from_index == 0u32.into() {
			// This is the case for finding an appropriate block number to start witnessing from

			let mut consensus: SupermajorityConsensus<_> = SupermajorityConsensus::default();

			for vote in consensus_votes.active_votes() {
				// we currently only count votes consisting of a single block height
				// there has to be a supermajority voting for the exact same header
				if vote.len() == 1 {
					consensus.insert_vote(vote[0].clone())
				}
			}

			Ok(consensus
				.check_consensus(&Threshold {
					threshold: success_threshold_from_share_count(num_authorities),
				})
				.map(|result| {
					let mut headers = VecDeque::new();
					headers.push_back(result);
					headers
				})
				.map(|result| {
					log::info!("block_height: initial consensus: {result:?}");
					result
				}))
		} else {
			// This is the actual consensus finding, once the engine is running

			let mut consensus: StagedConsensus<SupermajorityConsensus<Self::Consensus>, usize> =
				StagedConsensus::new();

			for mut vote in consensus_votes.active_votes() {
				// ensure that the vote is valid
				if let Err(err) = validate_vote_and_height(properties.witness_from_index, &vote) {
					log::warn!("received invalid vote: {err:?} ");
					continue;
				}

				// we count a given vote as multiple votes for all nonempty subchains
				while vote.len() > 0 {
					consensus.insert_vote((vote.len(), vote.clone()));
					vote.pop_back();
				}
			}

			Ok(consensus
				.check_consensus(&Threshold {
					threshold: success_threshold_from_share_count(num_authorities),
				})
				.map(|result| {
					log::info!(
						"(witness_from: {:?}): successful consensus for ranges: {:?}..={:?}",
						properties.witness_from_index,
						result.front(),
						result.back()
					);
					result
				}))
		}
	}
}

//-------- implementation of block height tracking as a state machine --------------

trait BlockHeightTrait : PartialEq
		+ Ord
		+ From<u32>
		+ Add<Self, Output = Self>
		+ Sub<Self, Output = Self>
		+ SubAssign<Self>
		+ AddAssign<Self>
		+ Copy {}

impl<A> BlockHeightTrait for A
	where A : PartialEq
		+ Ord
		+ From<u32>
		+ Add<Self, Output = Self>
		+ Sub<Self, Output = Self>
		+ SubAssign<Self>
		+ AddAssign<Self>
		+ Copy 
		{}


pub struct BlockHeightTrackingConsensus<
	ChainBlockNumber,
	ChainBlockHash,
> {
	votes: Vec<InputHeaders<ChainBlockHash, ChainBlockNumber>>
}


impl<
	ChainBlockNumber,
	ChainBlockHash,
> Default for BlockHeightTrackingConsensus<ChainBlockNumber, ChainBlockHash> {
    fn default() -> Self {
        Self {
			votes: Default::default()
		}
    }
}

impl<
	ChainBlockNumber : BlockHeightTrait + sp_std::fmt::Debug,
	ChainBlockHash : Clone + PartialEq + Ord + sp_std::fmt::Debug,
> Consensus for BlockHeightTrackingConsensus<ChainBlockNumber, ChainBlockHash> {
    type Vote = InputHeaders<ChainBlockHash, ChainBlockNumber>;
    type Result = InputHeaders<ChainBlockHash, ChainBlockNumber>;
    type Settings = (Threshold, ChainBlockNumber);

    fn insert_vote(&mut self, vote: Self::Vote) {
        self.votes.push(vote);
    }

    fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {

		// let num_authorities = consensus_votes.num_authorities();

		let (threshold, witness_from_index) = settings;

		if *witness_from_index == 0u32.into() {
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
				if let Err(err) = validate_vote_and_height(*witness_from_index, &vote.0) {
					log::warn!("received invalid vote: {err:?} ");
					continue;
				}

				// we count a given vote as multiple votes for all nonempty subchains
				while vote.0.len() > 0 {
					consensus.insert_vote((vote.0.len(), vote.clone()));
					vote.0.pop_back();
				}
			}

			consensus
				.check_consensus(&threshold)
				.map(|result| {
					log::info!(
						"(witness_from: {:?}): successful consensus for ranges: {:?}..={:?}",
						witness_from_index,
						result.0.front(),
						result.0.back()
					);
					result
				})
		}
    }
}




#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct InputHeaders<H,N>(pub VecDeque<Header<H,N>>);

impl<H,N: From<u32> + Copy + PartialEq> Fibered for InputHeaders<H,N> {
    type Base = N;

	fn is_in_fiber(&self, base: &Self::Base) -> bool {
		if *base == 0.into() {
			true
		} else {
			match self.0.front() {
				Some(first) => first.block_height == *base,
				None => false
			}
		}
		
	}
}

impl<H: PartialEq + Clone,N: BlockHeightTrait> Validate for InputHeaders<H,N>
{
	type Error = VoteValidationError;
    fn is_valid(&self) -> Result<(), Self::Error> {
		ChainBlocks {
			headers: self.0.clone(),
			next_height: 0.into()
		}.is_valid()
	}
}


impl<H,N: BlockHeightTrait> Validate for BlockHeightTrackingState<H,N>
where 
	H: PartialEq + Clone,
{
	type Error = VoteValidationError;
    fn is_valid(&self) -> Result<(), Self::Error> {
		self.headers.is_valid()
	}
}

impl<A, B: sp_std::fmt::Debug + Clone> Validate for Result<A,B> {
    type Error = B;

    fn is_valid(&self) -> Result<(), Self::Error> {
        match self {
            Ok(_) => Ok(()),
            Err(err) => Err(err.clone()),
        }
    }
}


#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum BHWState<H,N> {
	Starting,
	Running{headers: VecDeque<Header<H,N>>, witness_from: N}
}

impl<H,N> Default for BHWState<H,N> {
	fn default() -> Self {
		Self::Starting
	}
}

impl<H,N> Validate for BHWState<H,N> {
    type Error = &'static str;

    fn is_valid(&self) -> Result<(), Self::Error> {
        match self {
            BHWState::Starting => Ok(()),

			// TODO also check that headers are continuous
            BHWState::Running { headers, witness_from: _ } => if headers.len() > 0 { Ok(()) } else { Err("Block height tracking state should always be non-empty after start-up.") },
        }
    }
}

pub struct BlockHeightTrackingDSM<
	const SAFETY_MARGIN: usize,
	ChainBlockNumber,
	ChainBlockHash,
> {
	_phantom: core::marker::PhantomData<(ChainBlockNumber, ChainBlockHash)>,
}

impl<
	const SAFETY_MARGIN: usize,
	H: PartialEq + Eq + Clone + sp_std::fmt::Debug + 'static,
	N: BlockHeightTrait + sp_std::fmt::Debug + 'static,
> dependent_state_machine::Trait for BlockHeightTrackingDSM<SAFETY_MARGIN, N, H> 
{
    type State = BHWState<H,N>;
    type DisplayState = ChainProgress<N>;
    type Input = InputHeaders<H,N>;
    type Output = Result<ChainProgress<N>, &'static str>;

    fn request(s: &Self::State) -> <Self::Input as state_machine::Fibered>::Base {
        match s {
            BHWState::Starting => 0.into(),
            BHWState::Running { headers: _, witness_from } => witness_from.clone(),
        }
    }

    fn step(s: &mut Self::State, new_headers: Self::Input) -> Self::Output {

		match s {
			BHWState::Starting => {
				let first = new_headers.0.front().unwrap().block_height;
				let last  = new_headers.0.back().unwrap().block_height;
				*s = BHWState::Running { headers: new_headers.0.clone() , witness_from: last + 1.into()  };
				Ok(ChainProgress::Continuous(  first..=last ))
			},

			BHWState::Running { headers, witness_from } => {

				let mut chainblocks = ChainBlocks {
					headers: headers.clone(),
					next_height: headers.back().unwrap().block_height,
				};

				match chainblocks.merge(new_headers.0) {
					Ok(merge_info) => {
						log::info!(
							"added new blocks: {:?}, replacing these blocks: {:?}",
							merge_info.added,
							merge_info.removed
						);

						let safe_headers = trim_to_length(
							&mut chainblocks.headers,
							SAFETY_MARGIN,
						);

						*headers = chainblocks.headers;
						*witness_from = headers.back().unwrap().block_height + 1.into();

						// unsynchronised_state.last_safe_index +=
						// 	(safe_headers.len() as u32).into();

						// log::info!(
						// 	"the latest safe block height is {:?} (advanced by {})",
						// 	unsynchronised_state.last_safe_index,
						// 	safe_headers.len()
						// );

						Ok(merge_info.into_chain_progress().unwrap())
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
            BHWState::Running { headers, witness_from: _ } => ChainProgress::None( headers.back().unwrap().block_height ),
        }
    }
}
