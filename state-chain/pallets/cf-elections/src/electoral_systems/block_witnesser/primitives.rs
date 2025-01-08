use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use core::{iter::Step, ops::RangeInclusive};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::BTreeMap;

use crate::electoral_systems::state_machine::core::Validate;

/// Keeps track of ongoing elections for the block witnesser.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct ElectionTracker<N: Ord> {
	/// The next block height for which an election is going to be started once
	/// there is the capacity to do so and that height has been witnessed
	/// (`highest_scheduled >= next_election`).
	pub next_election: N,

	/// The highest block height that has been seen.
	pub highest_scheduled: N,

	/// Map containing all currently active elections.
	/// The associated usize is somewhat an artifact of the fact that
	/// this is intended to be used in an electoral system state machine. And the state machine
	/// has to know when to re-open an election which is currently ongoing.
	/// The state machine wouldn't close and reopen an election if the election properties
	/// stay the same, so we have (N, usize) as election properties. And when we want to reopen
	/// an ongoing election we increment the usize.
	pub ongoing: BTreeMap<N, u8>,

	/// When an election for a given block height is requested by inserting it in `ongoing`, it is
	/// always inserted with the current `reorg_id` as value.
	/// When a reorg is detected, this id is mutated to a new unique value that is not in
	/// `ongoing`. Elections in the range of a reorg are thus recreated with the new id, which
	/// causes them to be restarted by the electoral system.
	pub reorg_id: u8,
}

impl<N: Ord + Step + Copy> ElectionTracker<N> {
	/// Given the current state, if there are less than `max_ongoing`
	/// ongoing elections we push more elections into ongoing.
	pub fn start_more_elections(&mut self, max_ongoing: usize) {
		// filter out all elections which are ongoing, but shouldn't be, because
		// they are in the scheduled range (for example because there was a reorg)
		self.ongoing.retain(|height, _| *height < self.next_election);

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
		// If there is, we ensure that all ongoing or previously finished elections inside the reorg
		// range are going to be restarted once there is the capacity to do so.
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

impl<N: Ord + Step> Validate for ElectionTracker<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<N: BlockZero + Ord> Default for ElectionTracker<N> {
	fn default() -> Self {
		Self {
			highest_scheduled: BlockZero::zero(),
			next_election: BlockZero::zero(),
			ongoing: Default::default(),
			reorg_id: 0,
		}
	}
}

/// Generates an element which is not in `indices`.
fn generate_new_index<'a, N: BlockZero + Ord + Step + 'static>(
	mut indices: impl Iterator<Item = &'a N> + Clone,
) -> N {
	let mut index = N::zero();
	while indices.any(|ix| *ix == index) {
		index = N::forward(index, 1);
	}
	index
}

#[cfg(test)]
mod tests {

	use super::generate_new_index;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn indices_are_new(xs in prop::collection::vec(any::<u8>(), 0..3)) {
			assert!(!xs.contains(&generate_new_index(xs.iter())));
		}
	}
}
