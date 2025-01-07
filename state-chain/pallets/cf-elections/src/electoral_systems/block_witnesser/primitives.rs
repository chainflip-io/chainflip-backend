use core::{iter::Step, ops::RangeInclusive};
use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use frame_support::{ensure, Hashable};
use log::trace;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};
use sp_std::vec::Vec;
use sp_std::ops::Add;

use itertools::Either;

use crate::electoral_systems::block_height_tracking::state_machine::MultiIndexAndValue;
use crate::electoral_systems::block_height_tracking::{
	consensus::{ConsensusMechanism, SupermajorityConsensus, Threshold}, state_machine::{ConstantIndex, IndexOf, StateMachine, Validate}, state_machine_es::SMInput, ChainProgress
};
use crate::{SharedData, SharedDataHash};
use super::BlockWitnesserSettings;

// ----------------------------------- Election tracker -------------------------------------------------------
// Safe mode:
// when we enable safe mode, we want to take into account all reorgs,
// which means that we have to reopen elections for all elections which
// have been opened previously.
//
// This means that if safe mode is enabled, we don't call `start_more_elections`,
// but even in safe mode, if there's a reorg we call `restart_election`.

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize
)]
pub struct ElectionTracker<N: Ord> {
	/// The block heights which we have already received but not started elections for, yet.
	/// This means that we assume that we had elections for heights < scheduled.start().
	/// These might have already concluded of course.
	pub next_election: N,
	pub highest_scheduled: N,

	/// Map containing all currently active elections.
	/// The associated usize is somewhat an artifact of the fact that
	/// I intend this to be used in an ES state machine. And the state machine
	/// has to know when to re-open an election which is currently ongoing.
	/// The state machine wouldn't close and reopen an election if the election properties
	/// stay the same, so we have (N, usize) as election properties. And when we want to reopen
	/// an ongoing election we increment the usize.
	pub ongoing: BTreeMap<N, u8>,

	/// Whenever a reorg is detected, we increment this counter, as to restart all ongoing
	/// relevant elections.
	pub reorg_id: u8,
}

impl<N: Ord + Step + Copy> ElectionTracker<N> {
	/// Given the current state, if there are less than `max_ongoing`
	/// ongoing elections we push more elections into ongoing.
	pub fn start_more_elections(&mut self, max_ongoing: usize) {
		// filter out all elections which are ongoing, but shouldn't be, because
		// they are in the scheduled range (for example because there was a reorg)
		self.ongoing.retain(|height, _| *height < self.next_election );

		// schedule 
		while self.next_election <= self.highest_scheduled && self.ongoing.len() < max_ongoing {
			self.ongoing.insert(self.next_election, self.reorg_id);
			self.next_election = N::forward(self.next_election, 1);
		}
	}

	/// If an election is done we remove it from the ongoing list
	pub fn mark_election_done(&mut self, election: N) {
		if self.ongoing.remove(&election).is_none() {
			panic!("marking an election done which wasn't ongoing!")
		}
	}

	/// This function schedules all elections up to `range.end()`
	pub fn schedule_range(&mut self, range: RangeInclusive<N>) {

		// Check whether there is a reorg concerning elections we have started previously.
		// If there is, we ensure that all ongoing or previously finished elections inside the reorg range
		// are going to be restarted once there is the capacity to do so.
		if *range.start() < self.next_election {
			self.next_election = *range.start();
			self.reorg_id = generate_new_index(self.ongoing.values());
		}

		// QUESTION: currently, the following check ensures that
		// the highest scheduled election never decreases. Do we want this?
		// It's difficult to imagine a situation where the highest block number
		// after a reorg is lower than it was previously, and also, even if, in that
		// case we simply keep the higher number that doesn't seem to be too much of a problem.
		if self.highest_scheduled < *range.end() {
			self.highest_scheduled = *range.end();
		}
	}
}

impl<N : Ord + Step> Validate for ElectionTracker<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		ensure!(self.ongoing.iter().all(|(height, _)| height < &self.next_election),
			"ongoing elections should be < next_election"
		);
		Ok(())
	}
}

impl<N : BlockZero + Ord> Default for ElectionTracker<N> {
	fn default() -> Self {
		Self { highest_scheduled: BlockZero::zero(), next_election: BlockZero::zero(), ongoing: Default::default(), reorg_id: 0 }
	}
}

/// Generates an element which is not in `indices`.
fn generate_new_index<'a, N: BlockZero + Ord + Step + 'static>(mut indices: impl Iterator<Item = &'a N> + Clone) -> N {
	let mut index = N::zero();
	while indices.any(|ix| *ix == index) {
		index = N::forward(index, 1);
	}
	index
}

#[cfg(test)]
mod tests {

	use proptest::prelude::*;
	use super::generate_new_index;

	proptest!{
		#[test]
		fn indices_are_new(xs in prop::collection::vec(any::<u8>(), 0..3)) {
			assert!(!xs.contains(&generate_new_index(xs.iter())));
		}
	}
}
