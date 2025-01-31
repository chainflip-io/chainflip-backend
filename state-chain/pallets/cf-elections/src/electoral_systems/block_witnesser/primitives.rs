use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use core::{iter::Step, ops::RangeInclusive};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::BTreeMap;

use crate::electoral_systems::state_machine::core::Validate;

/// Keeps track of ongoing elections for the block witnesser.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct ElectionTracker<N: Ord> {
	/// The highest block height for which an election was started in the past.
	/// New elections are going to be started if there is the capacity to do so
	/// and that height has been witnessed (`highest_witnessed > highest_election`).
	pub highest_election: N,

	/// The highest block height that has been seen.
	pub highest_witnessed: N,

	/// The highest block height that we had previously started elections for and
	/// that was subsequently touched by a reorg.
	pub highest_priority: N,

	/// Map containing all currently active elections.
	/// The associated u8 "reord_id" is somewhat an artifact of the fact that
	/// this is intended to be used in an electoral system state machine. And the state machine
	/// has to know when to re-open an election which is currently ongoing.
	/// The state machine wouldn't close and reopen an election if the election properties
	/// stay the same, so we have (N, u8) as election properties. And when we want to reopen
	/// an ongoing election we change the u8 reorg_id.
	pub ongoing: BTreeMap<N, u8>,

	/// When an election for a given block height is requested by inserting it in `ongoing`, it is
	/// always inserted with the current `reorg_id` as value.
	/// When a reorg is detected, this id is mutated to a new unique value that is not in
	/// `ongoing`. Elections in the range of a reorg are thus recreated with the new id, which
	/// causes them to be restarted by the electoral system.
	pub reorg_id: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum SafeModeStatus {
	Enabled,
	Disabled,
}

impl<N: Ord + SaturatingStep + Step + Copy> ElectionTracker<N> {
	/// Given the current state, if there are less than `max_ongoing`
	/// ongoing elections we push more elections into ongoing.
	pub fn start_more_elections(&mut self, max_ongoing: usize, safemode: SafeModeStatus) {
		// In case of a reorg we still want to recreate elections for blocks which we had
		// elections for previously AND were touched by the reorg
		let start_up_to = match safemode {
			SafeModeStatus::Disabled => self.highest_witnessed,
			SafeModeStatus::Enabled => self.highest_priority,
		};

		// filter out all elections which are ongoing, but shouldn't be, because
		// they are in the scheduled range (for example because there was a reorg)
		self.ongoing.retain(|height, _| *height <= self.highest_election);

		// schedule
		for last_height in self.highest_election..start_up_to {
			let height = last_height.saturating_forward(1);
			if self.ongoing.len() < max_ongoing {
				self.ongoing.insert(height, self.reorg_id);
				self.highest_election = height;
			} else {
				break;
			}
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
		if *range.start() <= self.highest_election {
			// we set this value such that even in case of a reorg we create elections for up to
			// this block
			self.highest_priority = sp_std::cmp::max(self.highest_election, self.highest_priority);

			// the next election we start is going to be the first block involved in the reorg
			self.highest_election = range.start().saturating_backward(1);

			// and it's going to have a fresh `reorg_id` which forces the ES to recreate this
			// election
			self.reorg_id = generate_new_reorg_id(self.ongoing.values());
		}

		// QUESTION: currently, the following check ensures that
		// the highest scheduled election never decreases. Do we want this?
		// It's difficult to imagine a situation where the highest block number
		// after a reorg is lower than it was previously, and also, even if, in that
		// case we simply keep the higher number that doesn't seem to be too much of a problem.
		if self.highest_witnessed < *range.end() {
			self.highest_witnessed = *range.end();
		}
	}
}

impl<N: Ord> Validate for ElectionTracker<N> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<N: BlockZero + Ord> Default for ElectionTracker<N> {
	fn default() -> Self {
		Self {
			highest_witnessed: BlockZero::zero(),
			highest_priority: BlockZero::zero(),
			highest_election: BlockZero::zero(),
			ongoing: Default::default(),
			reorg_id: 0,
		}
	}
}

/// Generates an element which is not in `indices`.
fn generate_new_reorg_id<'a, N: BlockZero + SaturatingStep + Ord + 'static>(
	mut indices: impl Iterator<Item = &'a N> + Clone,
) -> N {
	let mut index = N::zero();
	while indices.any(|ix| *ix == index) {
		index = index.saturating_forward(1);
	}
	index
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgressInner<ChainBlockNumber: Step> {
	Progress(ChainBlockNumber),
	Reorg(RangeInclusive<ChainBlockNumber>),
}

#[cfg(test)]
mod tests {

	use super::generate_new_reorg_id;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn indices_are_new(xs in prop::collection::vec(any::<u8>(), 0..3)) {
			assert!(!xs.contains(&generate_new_reorg_id(xs.iter())));
		}
	}
}
