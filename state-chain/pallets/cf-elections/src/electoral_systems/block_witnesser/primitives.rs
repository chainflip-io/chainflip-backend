use core::{iter::Step, ops::RangeInclusive};
use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use frame_support::{ensure, Hashable};
use log::trace;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};
use sp_std::vec::Vec;

use itertools::Either;

use crate::electoral_systems::block_height_tracking::state_machine::IndexAndValue;
use crate::electoral_systems::block_height_tracking::{
	consensus::{ConsensusMechanism, SupermajorityConsensus, Threshold}, state_machine::{ConstantIndex, IndexOf, StateMachine, Validate}, state_machine_es::SMInput, ChainProgress
};
use crate::{SharedData, SharedDataHash};

use super::BlockWitnesserSettings;

// Safe mode:
// when we enable safe mode, we want to take into account all reorgs,
// which means that we have to reopen elections for all elections which
// have been opened previously.
//
// This means that if safe mode is enabled, we don't call `start_more_elections`,
// but even in safe mode, if there's a reorg we call `restart_election`.

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
struct ElectionTracker<N: Ord> {
	pub highest_scheduled: N,
	pub highest_started: N,

	/// Map containing all currently active elections.
	/// The associated usize is somewhat an artifact of the fact that
	/// I intend this to be used in an ES state machine. And the state machine
	/// has to know when to re-open an election which is currently ongoing.
	/// The state machine wouldn't close and reopen an election if the election properties
	/// stay the same, so we have (N, usize) as election properties. And when we want to reopen
	/// an ongoing election we increment the usize.
	pub ongoing: BTreeMap<N, u32>,
}

impl<N: Ord + Step + Copy> ElectionTracker<N> {
	/// Given the current state, if there are less than `max_ongoing`
	/// ongoing elections we push more elections into ongoing.
	pub fn start_more_elections(&mut self, max_ongoing: usize) {
		while self.highest_started < self.highest_scheduled && self.ongoing.len() < max_ongoing {
			self.highest_started = N::forward(self.highest_started, 1);
			self.ongoing.insert(self.highest_started, 0);
		}
	}

	/// If an election is done we remove it from the ongoing list
	pub fn mark_election_done(&mut self, election: N) {
		if self.ongoing.remove(&election).is_none() {
			panic!("marking an election done which wasn't ongoing!")
		}
	}

	/// This function only restarts elections which have been previously
	/// started (i.e. <= highest started).
	pub fn restart_election(&mut self, election: N) {
		if election <= self.highest_started {
			*self.ongoing.entry(election).or_insert(0) += 1;
		}
	}

	/// This function schedules all elections up to `election`
	pub fn schedule_up_to(&mut self, election: N) {
		if self.highest_scheduled < election {
			self.highest_scheduled = election;
		}
	}
}

impl<N : Ord> Validate for ElectionTracker<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		ensure!(self.highest_started > self.highest_scheduled,
			"highest_started should be <= highest_scheduled"
		);
		ensure!(self.ongoing.iter().any(|(height, _)| height > &self.highest_started),
			"ongoing elections should be <= highest_started"
		);
		Ok(())
	}
}

impl<N : BlockZero + Ord> Default for ElectionTracker<N> {
	fn default() -> Self {
		Self { highest_scheduled: BlockZero::zero(), highest_started: BlockZero::zero(), ongoing: Default::default() }
	}
}


#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWSettings {
	pub safe_mode_enabled: bool,
	pub max_concurrent_elections: u32,
}

// #[derive(
// 	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
// )]
// pub struct BWHooks<N, InputIndex> {
// 	pub active_deposit_channels_at: Box<dyn FnMut(N) -> InputIndex>
// }

pub trait BWHooks<N, InputIndex> {
	fn active_deposit_channels_at(height: N) -> InputIndex;
}

pub trait Hook<A,B> {
	fn run(input: A) -> B;
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWState<N: Ord> {
	elections: ElectionTracker<N>,
}

impl<N: Ord> Validate for BWState<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.elections.is_valid()
	}
}

impl<N: BlockZero + Ord> Default for BWState<N> {
	fn default() -> Self {
		Self { elections: Default::default() }
	}
}

pub struct BWStateMachine<
	ElectionProperties,
	BlockData,
	N,
	BlockElectionPropertiesGenerator: Hook<N, ElectionProperties>
	> {
	_phantom: sp_std::marker::PhantomData<(ElectionProperties, BlockData, N, BlockElectionPropertiesGenerator)>,
}

impl<
	ElectionProperties: PartialEq + Clone + sp_std::fmt::Debug + 'static,
	BlockData: PartialEq + Clone + sp_std::fmt::Debug + 'static,
	N : Copy + Ord + Step + sp_std::fmt::Debug + 'static,
	BlockElectionPropertiesGenerator: Hook<N, ElectionProperties> + 'static
> StateMachine for BWStateMachine<ElectionProperties, BlockData, N, BlockElectionPropertiesGenerator> {
	type Input = SMInput<IndexAndValue<(N, ElectionProperties, u32), BlockData>, ChainProgress<N>>;

	type Settings = BWSettings;

	type Output = Result<(), &'static str>;

	type State = BWState<N>;

	fn input_index(s: &Self::State) -> Vec<IndexOf<Self::Input>> {
		s.elections.ongoing.clone().into_iter().map(|(height, extra)| (height, BlockElectionPropertiesGenerator::run(height), extra)).collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, settings: &Self::Settings) -> Self::Output {
		log::info!("BW: input {i:?}");
		match i {
			SMInput::Context(ChainProgress::Reorg(range) | ChainProgress::Continuous(range)) => {
				s.elections.schedule_up_to(*range.end());
				for election in range {
					s.elections.restart_election(election);
				}
			},

			SMInput::Context(ChainProgress::WaitingForFirstConsensus | ChainProgress::None(_)) => {},

			SMInput::Vote(blockdata) => {
				// insert blockdata into our cache of blocks
				s.elections.mark_election_done(blockdata.0.0);
				log::info!("got block data: {:?}", blockdata.1);
			},
		};

		if !settings.safe_mode_enabled {
			s.elections.start_more_elections(settings.max_concurrent_elections as usize);
		}

		log::info!("BW: done. current elections: {:?}", s.elections.ongoing);

		Ok(())
	}

}

pub struct BWConsensus<BlockData: Eq, N, ElectionProperties> {
	pub consensus: SupermajorityConsensus<SharedDataHash>,
	pub data: BTreeMap::<SharedDataHash, BlockData>,
	pub _phantom: sp_std::marker::PhantomData<(N, ElectionProperties)>
}

impl<BlockData: Eq, N, ElectionProperties> Default for BWConsensus<BlockData, N, ElectionProperties> {
	fn default() -> Self {
		Self { consensus: Default::default(), data: Default::default(), _phantom: Default::default() }
	}
}

impl<BlockData: Eq + Clone + sp_std::fmt::Debug + Hashable, N: Clone, ElectionProperties: Clone> ConsensusMechanism for BWConsensus<BlockData, N, ElectionProperties> {
	type Vote = ConstantIndex<(N, ElectionProperties, u32), BlockData>;

	type Result = IndexAndValue<(N, ElectionProperties, u32), BlockData>;

	type Settings = (Threshold, (N, ElectionProperties, u32));

	fn insert_vote(&mut self, vote: Self::Vote) {
		let vote_hash = SharedDataHash::of(&vote.data);
		self.data.insert(vote_hash, vote.data.clone());
		self.consensus.insert_vote(vote_hash);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		self.consensus.check_consensus(&settings.0)
			.map(|consensus| self.data.get(&consensus).expect("hash of vote should exist").clone())
			.map(|data| IndexAndValue(settings.1.clone(), data))
	}
}

