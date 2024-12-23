use core::{iter::Step, ops::RangeInclusive};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};

use itertools::Either;

use crate::electoral_systems::block_height_tracking::{
	state_machine::{IndexOf, StateMachine},
	ChainProgress,
};

// Safe mode:
// when we enable safe mode, we want to take into account all reorgs,
// which means that we have to reopen elections for all elections which
// have been opened previously.
//
// This means that if safe mode is enabled, we don't call `start_more_elections`,
// but even in safe mode, if there's a reorg we call `restart_election`.

struct ElectionTracker<N> {
	pub highest_scheduled: N,
	pub highest_started: N,

	/// Map containing all currently active elections.
	/// The associated usize is somewhat an artifact of the fact that
	/// I intend this to be used in an ES state machine. And the state machine
	/// has to know when to re-open an election which is currently ongoing.
	/// The state machine wouldn't close and reopen an election if the election properties
	/// stay the same, so we have (N, usize) as election properties. And when we want to reopen
	/// an ongoing election we increment the usize.
	pub ongoing: BTreeMap<N, usize>,
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

/// Mock data
struct BlockData<N> {
	height: N,
}

struct BWSettings {
	safe_mode_enabled: bool,
	max_elections: usize,
}

struct BWState<N> {
	elections: ElectionTracker<N>,
}

struct BWStateMachine<N> {
	_phantom: sp_std::marker::PhantomData<N>,
}

// impl<N : Ord + Step> StateMachine for BWStateMachine<N> {

type Input = (Either<ChainProgress<u64>, BlockData<u64>>, BWSettings);
type Output = ();
type State = BWState<u64>;
type DisplayState = ();

fn input_index(s: &State) -> BTreeSet<u64> {
	todo!()
}

fn step(s: &mut State, (i, settings): Input) -> Output {
	match i {
		Either::Left(ChainProgress::Reorg(range) | ChainProgress::Continuous(range)) => {
			s.elections.schedule_up_to(*range.end());
			for election in range {
				s.elections.restart_election(election);
			}
		},

		Either::Left(ChainProgress::WaitingForFirstConsensus | ChainProgress::None(_)) => {},

		Either::Right(blockdata) => {
			// insert blockdata into our cache of blocks
			todo!()
		},
	}

	if !settings.safe_mode_enabled {
		s.elections.start_more_elections(settings.max_elections);
	}
}

fn get(s: &State) -> DisplayState {
	()
}

// }
