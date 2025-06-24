use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use core::{
	cmp::min,
	iter::Step,
	ops::{Range, RangeInclusive},
};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	cmp::max,
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

#[cfg(test)]
use proptest::prelude::{any, Arbitrary, Strategy};
#[cfg(test)]
use proptest::proptest;
#[cfg(test)]
use proptest_derive::Arbitrary;

use crate::electoral_systems::{
	block_height_witnesser::{ChainBlockHashOf, ChainBlockNumberOf, ChainProgress, ChainTypes},
	state_machine::core::{def_derive, defx, Hook, Validate},
};

use super::state_machine::{BWElectionType, BWTypes, EngineElectionType};

defx! {
	pub struct ElectionTracker[T: BWTypes] {
		/// The lowest block we haven't seen yet. I.e., we have seen blocks below.
		pub seen_heights_below: ChainBlockNumberOf<T::Chain>,

		/// Since the boundary between ongoing and queued_elections is fuzzy (due to reorgs currently
		/// ongoing elections might turn into scheduled ones), we're separately keeping track of the
		/// block height we have ever scheduled elections for. In case of reorgs into safe-mode we're
		/// always going to reschedule elections for heights up to this one.
		pub highest_ever_ongoing_election: ChainBlockNumberOf<T::Chain>,

		/// Block hashes we got from the BHW.
		pub queued_hash_elections: BTreeMap<ChainBlockNumberOf<T::Chain>, ChainBlockHashOf<T::Chain>>,

		/// Block heights which are queued but already past the safetymargin don't
		/// have associated hashes. We just store a list of block height ranges.
		pub queued_safe_elections: CompactHeightTracker<ChainBlockNumberOf<T::Chain>>,

		/// Hashes of elections currently ongoing
		pub ongoing: BTreeMap<ChainBlockNumberOf<T::Chain>, BWElectionType<T>>,

		/// Optimistic blocks
		pub optimistic_block_cache: BTreeMap<ChainBlockNumberOf<T::Chain>, OptimisticBlock<T>>,

		/// debug hook
		pub debug_events: T::ElectionTrackerDebugEventHook,
	}

	validate this (else ElectionTrackerError) {

		//--- highest block is updated ---
		// The `seen_heights_below` value should always be one more than the highest seen block.
		// This property takes into account all queued and ongoing elections, except "optimistic",
		// since those are for blocks that we haven't seen yet.
		seen_heights_below_is_updated: {
			this.queued_hash_elections.keys()
			.chain(this.queued_safe_elections.get_all_heights().iter())
			.chain(this.ongoing.iter().filter(|(_, election_type)| **election_type != BWElectionType::Optimistic).map(|(height, _)| height))
			.max()
			.cloned()
			.map(|max_height| max_height.saturating_forward(1) <= this.seen_heights_below)
			.unwrap_or(true)
		},

		// The `highest_ever_ongoing_election` should always be updated when new elections are created.
		highest_ever_ongoing_election_is_updated: {
			this.ongoing.keys().all(|height| *height <= this.highest_ever_ongoing_election)
		},

		//--- ensure that we delete old data ---
		// We should only store data received from optimistic elections for at most SAFETY_BUFFER blocks.
		optimistic_block_cache_is_cleared: this.optimistic_block_cache.iter().all(|(height, _block)|
			height.saturating_forward(T::Chain::SAFETY_BUFFER) > this.seen_heights_below
		),

		//--- disjointness of all elections ---
		// The following properties verify that the three sets of
		//  - ongoing,
		//  - queued by hash,
		//  - and queued by height
		// elections are pairwise disjoint. They also ensure that they
		// adhere to the following ordering:
		//
		// |--- ongoing ---/--- queued by height ---|--- queued by hash ---||
		//                                          |<-- SAFETY_BUFFER  -->||
		//                                                                  ^- seen_heights_below
		// >> increasing block heights >>
		//
		// Unfortunately, we cannot guarantee that the ongoing elections are always lower than the
		// queued by height. This would happen if we receive heights from the BHW
		// that are over SAFETY_BUFFER in the past. In the current implementation of the BHW this is impossible.
		//
		// The proptests still generate this kind of input, so we don't enforce an ordering of ongoing vs queued by height
		// elections. We do ensure that they are disjoint.

		ongoing_elections_are_lower_than_queued: {
			if let Some(highest_ongoing) = this.ongoing.keys().max().cloned() {
				this.queued_hash_elections.keys().all(|height| highest_ongoing < *height)
			} else {
				true
			}
		},

		ongoing_and_queued_by_height_are_disjoint:
			this.ongoing.keys().cloned().collect::<BTreeSet<_>>().intersection(
				&this.queued_safe_elections.get_all_heights()).count() == 0,

		elections_queued_by_hash_are_inside_safety_buffer:
			this.queued_hash_elections.keys().all(
				|height| height.saturating_forward(T::Chain::SAFETY_BUFFER) >= this.seen_heights_below
			),

		elections_queued_by_safe_are_outside_safety_buffer:
			this.queued_safe_elections.get_all_heights().iter().all(
				|height| height.saturating_forward(T::Chain::SAFETY_BUFFER) < this.seen_heights_below
			)
	}
}

impl<T: BWTypes> ElectionTracker<T> {
	pub fn start_more_elections(
		&mut self,
		max_ongoing: usize,
		max_optimistic: u8,
		safemode: SafeModeStatus,
	) {
		use BWElectionType::*;

		// First we remove all Optimistic elections, we're going to recreate them if needed.
		// This ensures that ongoing optimistic elections don't block more important ByHash
		// elections.
		self.ongoing.retain(|_, election_type| *election_type != Optimistic);

		// schedule at most `max_new_elections`
		let max_new_elections = max_ongoing.saturating_sub(self.ongoing.len());

		let opti_elections = (self.seen_heights_below..
			self.seen_heights_below.saturating_forward(max_optimistic as usize))
			.map(|x| (x, Optimistic));

		let all_block_heights = self
			.queued_safe_elections
			.get_all_heights()
			.into_iter()
			.chain(self.queued_hash_elections.keys().cloned())
			.chain(opti_elections.clone().map(|(height, _)| height));

		let new_elections_count = all_block_heights
			.take_while(|height| {
				safemode == SafeModeStatus::Disabled ||
					*height <= self.highest_ever_ongoing_election
			})
			.take(max_new_elections)
			.count();

		let safe_elections = self
			.queued_safe_elections
			.extract_lazily()
			.map(|height| (height, SafeBlockHeight));

		let hash_elections = self
			.queued_hash_elections
			.extract_if(|_, _| true)
			.map(|(height, hash)| (height, ByHash(hash)));

		self.ongoing.extend(
			safe_elections
				.chain(hash_elections)
				.chain(opti_elections)
				.take(new_elections_count),
		);

		// Make sure that we always update the highest ever ongoing election after we have scheduled
		// new ones
		self.ongoing.last_key_value().inspect(|(height, _)| {
			self.highest_ever_ongoing_election = max(self.highest_ever_ongoing_election, **height);
		});

		// clean up the optimistic block cache
		self.optimistic_block_cache.retain(|height, _| {
			height.saturating_forward(T::Chain::SAFETY_BUFFER) > self.seen_heights_below
		});
	}

	/// If an election is done we remove it from the ongoing list.
	/// This function returns the current election type for the given block height.
	/// Note that the election type might be different from the election type that was closed for
	/// this height, due to:
	///  - We had a reorg, and the hash we are querying for changed, and in the same statechain
	///    block we receive a result for the old hash
	///  - The election was `Optimistic`, we received a hash for it, and in the same statechain
	///    block we got the result of the optimistic election, which might or might not be for the
	///    hash we now got.
	///  - The election was `ByHash`, but it got too old and its type changed to `SafeBlockHeight`
	pub fn mark_election_done(
		&mut self,
		height: ChainBlockNumberOf<T::Chain>,
		received: &EngineElectionType<T::Chain>,
		received_hash: &Option<ChainBlockHashOf<T::Chain>>,
		received_data: T::BlockData,
	) -> Option<T::BlockData> {
		// update the lowest unseen block,
		// currently this only has an effect if we get an Optimistic block
		self.seen_heights_below = max(self.seen_heights_below, height.saturating_forward(1));

		// Note: if we receive blockdata for a block number, and
		// in the same statechain block there's a reorg which changes the hash of this block,
		// then we shouldn't close the election.

		use BWElectionType::*;
		self.ongoing
			.get(&height)
			.cloned()
			.and_then(|current| {
				self.debug_events.run(ElectionTrackerEvent::ComparingBlocks {
					height,
					hash: received_hash.clone(),
					received: received.clone(),
					current: current.clone(),
				});

				match (received, &current) {
					//---------------------------
					// if we receive a result for the same election type as is currently open, we
					// close it. There are 4 cases here, and we close the ongoing election if
					// it matches our current query.

					// case 1 (optimistic)
					(EngineElectionType::BlockHeight { submit_hash: true }, Optimistic) =>
						Some(current),

					// case 2 (by hash):
					// if we get consensus for a by-hash election whose hash doesn't match with
					// the hash we have currently, we keep it open
					(EngineElectionType::ByHash(a), ByHash(b)) =>
						if a == b {
							Some(current)
						} else {
							None
						},

					// case 3 (safe height):
					(EngineElectionType::BlockHeight { submit_hash: false }, SafeBlockHeight) =>
						Some(current),

					// case 4 (governance):
					// these are treated the same as safe height
					(EngineElectionType::BlockHeight { submit_hash: false }, Governance(_)) =>
						Some(current),

					//---------------------------
					// if we receive another result for an optimistic election (with hash
					// submission), there are 3 cases to consider.
					//

					// case 1 (optimistic election changed into by-hash):
					// if we get an optimistic consensus for an election that is already by-hash,
					// we check whether the `received_hash` is the same as the hash we're currently
					// querying for. If it is, we accept the optimistic block as result for the
					// by-hash election. otherwise we keep the by-hash election open.
					(
						EngineElectionType::BlockHeight { submit_hash: true },
						ByHash(current_hash),
					) =>
						if received_hash.as_ref() == Some(current_hash) {
							Some(current)
						} else {
							None
						},

					// case 2 (optimistic election changed into safe block height):
					// If we get an optimistic consensus for an election that is already past
					// safety-margin we ignore it, it's safer to re-query by block height. This
					// should virtually never happen, only in case where the querying takes a *very*
					// long time.
					(EngineElectionType::BlockHeight { submit_hash: true }, SafeBlockHeight) =>
						None,

					// case 3 (optimistic election changed into governance election):
					// this should not be possible, so we ignore the election result.
					(EngineElectionType::BlockHeight { submit_hash: true }, Governance(_)) => None,

					//---------------------------
					// if we receive another result for a by-hash election, there are 2 cases to
					// consider:

					// case 1 (by-hash election changed into safe height):
					// If we get a by-hash consensus for an election that is already past
					// safety-margin, we ignore it. We've already deleted the hash for this
					// election from storage, so we can't check whether we got the correct
					// block. It's safer to re-query.
					(EngineElectionType::ByHash(_), SafeBlockHeight) => None,

					// case 2 (by-hash election changed into governance election):
					// this should not be possible, so we ignore the election result.
					(EngineElectionType::ByHash(_), Governance(_)) => None,

					//---------------------------
					// the following 3 cases should be impossible

					// case 1 and 2 (election changes back into optimistic)
					// both are impossible since hash and block height
					// elections cannot change (back) into optimistic elections.
					// This is currently guaranteed by the fact that we only start Optimistic
					// elections starting from `seen_heights_below` and this value is
					// monotonically increasing.
					(EngineElectionType::ByHash(_), Optimistic) => None,
					(EngineElectionType::BlockHeight { submit_hash: false }, Optimistic) => None,

					// case 3 (election changes back into by-hash)
					// similar to the previous two, the boundary between safe-height and by-hash
					// elections is demarked by (`seen_heights_below - SAFETY_BUFFER`). Since this
					// value is monotonically increasing, once an election is scheduled
					// safe-height it cannot change back to being ByHash.
					(EngineElectionType::BlockHeight { submit_hash: false }, ByHash(_)) => None,
				}
			})
			.inspect(|_| {
				self.ongoing.remove(&height);
			})
			.and_then(|t| match t {
				// we closed an optimistic election, this means we don't have the hash yet
				// so we include the block in our optimistic cache
				Optimistic => {
					self.optimistic_block_cache.insert(
						height,
						OptimisticBlock {
							hash: received_hash.clone().unwrap(),
							data: received_data,
						},
					);
					None
				},
				// Otherwise we know that this block is correct and can be forwarded to the
				// block processor, thus we return it here.
				ByHash(_) | SafeBlockHeight | Governance(_) => Some(received_data),
			})
	}

	/// This function schedules all elections up to the last block_height we've seen + 1 (for
	/// optimistic block)
	pub fn schedule_range(
		&mut self,
		progress: ChainProgress<T::Chain>,
	) -> Vec<(ChainBlockNumberOf<T::Chain>, OptimisticBlock<T>)> {
		// If there was a reorg, remove any references to the reorged heights
		// in the election tracker.
		if let Some(ref removed) = progress.removed {
			self.queued_safe_elections
				.remove(*removed.start()..removed.end().saturating_forward(1));
			self.queued_hash_elections.retain(|height, _| !removed.contains(height));
			self.ongoing.retain(|height, _| !removed.contains(height));
		}

		let last_seen_height = progress.headers.last().block_height;

		// We definitely want to ensure that `self.seen_heights_below` is monotonically
		// increasing in order to have saner invariants hold for the other components
		// of the election tracker.
		self.seen_heights_below =
			max(self.seen_heights_below, last_seen_height.saturating_forward(1));

		let (accepted_optimistic_blocks, mut remaining): (BTreeMap<_, _>, BTreeMap<_, _>) =
			progress.headers.headers.into_iter().fold(
				(BTreeMap::new(), BTreeMap::new()),
				|(mut optimistic_blocks, mut remaining), header| {
					match self.optimistic_block_cache.remove(&header.block_height) {
						Some(optimistic_block) if optimistic_block.hash == header.hash => {
							optimistic_blocks.insert(header.block_height, optimistic_block);
						},
						_ => {
							remaining.insert(header.block_height, header.hash.clone());
						},
					}
					(optimistic_blocks, remaining)
				},
			);

		// add all remaining hashes to the queue
		self.queued_hash_elections.append(&mut remaining);

		let is_safe_height = |height: &ChainBlockNumberOf<T::Chain>| {
			height.saturating_forward(T::Chain::SAFETY_BUFFER) < self.seen_heights_below
		};

		// clean up the queue by removing old hashes
		self.queued_hash_elections
			.extract_if(|height, _| is_safe_height(height))
			.for_each(|(height, _)| {
				self.queued_safe_elections.insert(height);
			});

		// move ongoing elections from ByHash to SafeBlockHeight if they become old enough
		self.ongoing.iter_mut().for_each(|(height, ty)| {
			if is_safe_height(height) && matches!(ty, BWElectionType::ByHash(_)) {
				*ty = BWElectionType::SafeBlockHeight;
			}
		});

		accepted_optimistic_blocks.into_iter().collect()
	}
	pub fn lowest_in_progress_height(&self) -> ChainBlockNumberOf<T::Chain> {
		*self
			.ongoing
			.iter()
			.filter(|(_, t)| **t != BWElectionType::Optimistic)
			.map(|(h, _)| h)
			.chain(self.queued_hash_elections.keys())
			.chain(self.queued_safe_elections.get_all_heights().iter())
			.min()
			.unwrap_or(&self.seen_heights_below)
	}
}

impl<T: BWTypes> Default for ElectionTracker<T> {
	fn default() -> Self {
		Self {
			seen_heights_below: ChainBlockNumberOf::<T::Chain>::default(),
			highest_ever_ongoing_election: ChainBlockNumberOf::<T::Chain>::default(),
			queued_hash_elections: Default::default(),
			ongoing: Default::default(),
			queued_safe_elections: Default::default(),
			optimistic_block_cache: Default::default(),
			debug_events: Default::default(),
		}
	}
}

#[cfg(test)]
impl<T: BWTypes> Arbitrary for ElectionTracker<T> {
	type Parameters = ();

	fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
		use crate::prop_do;
		use proptest::{
			collection::{btree_map, btree_set, vec},
			prelude::Just,
		};

		let chain_block_number = || {
			(0..(u32::MAX / 2) as usize).prop_map(|delta| {
				<ChainBlockNumberOf<T::Chain> as Default>::default().saturating_forward(delta)
			})
		};

		prop_do!(
			let ongoing_below in chain_block_number();
			let highest_seen_delta in 1..1000usize;
			let highest_ongoign_delta in 0..1000usize;
			let heights in btree_set(chain_block_number(), 0..40);
			let currently_highest = heights.iter().max().cloned().unwrap_or(Default::default());
			let seen_heights_below = currently_highest.saturating_forward(highest_seen_delta);
			let highest_ever_ongoing = currently_highest.saturating_forward(highest_ongoign_delta);
			let safe_elections_below = max(ongoing_below, seen_heights_below.saturating_backward(T::Chain::SAFETY_BUFFER));
			let ongoing = heights.iter().filter(|h| **h < ongoing_below).cloned().collect::<BTreeSet<_>>();
			let safe = heights.iter().filter(|h| (ongoing_below..safe_elections_below).contains(*h)).cloned().collect::<BTreeSet<_>>();
			let hash = heights.iter().filter(|h| (safe_elections_below..seen_heights_below).contains(*h)).cloned().collect::<BTreeSet<_>>();

			Self {
				seen_heights_below: Just(seen_heights_below),
				highest_ever_ongoing_election: Just(highest_ever_ongoing),
				ongoing: vec(any::<BWElectionType<T>>(), ongoing.len()..=ongoing.len())
					.prop_map(move |types|
						ongoing.clone()
							.into_iter()
							.zip(types.into_iter())
							.collect::<BTreeMap<_,_>>()
					),
				queued_safe_elections: Just(
					safe.into_iter().fold(CompactHeightTracker::new(), |mut acc, height| {
						acc.insert(height);
						acc
					})
				),
				queued_hash_elections: vec(any::<ChainBlockHashOf<T::Chain>>(), hash.len()..=hash.len())
					.prop_map(move |hashes|
						hash.clone()
							.into_iter()
							.zip(hashes.into_iter())
							.collect::<BTreeMap<_,_>>()
					),
				optimistic_block_cache: btree_map(
						any::<ChainBlockNumberOf<T::Chain>>(),
						any::<OptimisticBlock<T>>(),
						0..10
					).prop_map(move |mut blocks| {
						blocks.retain(|height, _| height.saturating_forward(T::Chain::SAFETY_BUFFER) > seen_heights_below);
						blocks
					}),
				debug_events: Just(Default::default()),
			}
		)
	}

	type Strategy = impl Strategy<Value = Self> + Clone + Sync + Send;
}

def_derive! {
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct OptimisticBlock<T: BWTypes> {
		pub hash: <T::Chain as ChainTypes>::ChainBlockHash,
		pub data: T::BlockData,
	}
}
impl<T: BWTypes> Validate for OptimisticBlock<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

def_derive! {
	pub enum ElectionTrackerEvent<T: BWTypes> {
		ComparingBlocks {
			height: ChainBlockNumberOf<T::Chain>,
			hash: Option<ChainBlockHashOf<T::Chain>>,
			received: EngineElectionType<T::Chain>,
			current: BWElectionType<T>,
		},
		UpdateSafeElections {
			old: CompactHeightTracker<ChainBlockNumberOf<T::Chain>>,
			new: CompactHeightTracker<ChainBlockNumberOf<T::Chain>>,
			reason: UpdateSafeElectionsReason,
		},
	}
}

def_derive! {
	pub enum UpdateSafeElectionsReason {
		OutOfSafetyMargin,
		SafeElectionScheduled,
		GotOptimisticBlock,
		ReorgReceived,
	}
}

/// There is some assumption regardint CompactHeightTracker:
/// - we can only insert at the end
#[derive_where(Default; )]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct CompactHeightTracker<N: Ord> {
	elections: BTreeMap<N, N>,
}
impl<N: Ord> Validate for CompactHeightTracker<N> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<N: Clone + SaturatingStep + Ord> CompactHeightTracker<N> {
	pub fn new() -> Self {
		Self { elections: Default::default() }
	}

	pub fn extract_lazily(&mut self) -> CompactHeightTrackerExtract<'_, N> {
		CompactHeightTrackerExtract { tracker: self }
	}

	pub fn insert(&mut self, item: N) {
		if let Some(mut last) = self.elections.last_entry() {
			if *last.get() == item {
				last.insert(N::saturating_forward(item, 1));
				return;
			}
		}
		self.elections.insert(item.clone(), N::saturating_forward(item, 1));
	}

	pub fn remove(&mut self, remove: Range<N>) {
		self.elections = self
			.elections
			.clone()
			.into_iter()
			.filter_map(|(mut start, mut end)| {
				if end <= remove.end {
					end = min(end, remove.start.clone());
				}
				if start >= remove.start {
					start = max(start.clone(), remove.end.clone());
				}
				if start < end {
					Some((start, end))
				} else {
					None
				}
			})
			.collect();
		if let Some(big_range) = self
			.elections
			.iter()
			.find(|(a, b)| **a < remove.start && remove.end < **b)
			.map(|(a, b)| (a.clone(), b.clone()))
		{
			self.elections.remove(&big_range.0);
			self.elections.insert(big_range.0, remove.start);
			self.elections.insert(remove.end, big_range.1);
		}
	}

	pub fn get_all_heights(&self) -> BTreeSet<N>
	where
		N: Step,
	{
		self.elections.iter().flat_map(|(a, b)| (a.clone()..b.clone())).collect()
	}
}

pub struct CompactHeightTrackerExtract<'a, N: Ord> {
	tracker: &'a mut CompactHeightTracker<N>,
}

impl<N: SaturatingStep + Ord + Clone> Iterator for CompactHeightTrackerExtract<'_, N> {
	type Item = N;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(first) = self.tracker.elections.first_entry() {
			let (start, end) = first.remove_entry();
			let result = start.clone();
			let start = start.saturating_forward(1);
			if start < end {
				self.tracker.elections.insert(start, end);
			}
			Some(result)
		} else {
			None
		}
	}
}

#[test]
pub fn test_compact_height() {
	let mut tracker = CompactHeightTracker::new();
	tracker.insert(10u8);
	tracker.insert(20u8);
	assert_eq!(tracker.get_all_heights(), [10, 20].into_iter().collect());
	{
		let mut iter = tracker.extract_lazily();
		assert_eq!(iter.next(), Some(10));
	}
	assert_eq!(tracker.elections, [(20, 21)].into_iter().collect());

	let mut tracker = CompactHeightTracker::new();
	tracker.insert(1u8);
	tracker.insert(2u8);
	tracker.insert(3u8);
	tracker.insert(4u8);
	tracker.insert(5u8);
	tracker.remove(2..4);
	assert_eq!(tracker.get_all_heights(), [1, 4, 5].into_iter().collect());

	let mut tracker = CompactHeightTracker::new();
	tracker.insert(1u8);
	tracker.insert(2u8);
	tracker.insert(3u8);
	tracker.insert(4u8);
	tracker.insert(5u8);
	tracker.remove(1..6);
	assert_eq!(tracker.elections, [].into_iter().collect());
}

#[cfg(test)]
mod prop_tests {
	use super::*;
	use proptest::prelude::*;
	use std::collections::BTreeSet;

	#[derive(Debug, Clone)]
	enum TrackerOp {
		Insert(u8),
		Remove { start: u8, end: u8 },
	}

	fn tracker_ops_strategy() -> impl Strategy<Value = Vec<TrackerOp>> {
		// First, generate a set of unique inserts
		prop::collection::btree_set(any::<u8>(), 1..100).prop_flat_map(|insert_set| {
			let inserts: Vec<_> = insert_set
				.iter()
				.cloned()
				.map(|n| TrackerOp::Insert(n.saturating_sub(1)))
				.collect();
			// Now, generate removes that only target present elements
			let removes_strategy = {
				let vec: Vec<u8> = insert_set.into_iter().collect();
				prop::collection::vec(
					(0..vec.len()).prop_flat_map(move |i| {
						let start = vec[i];
						let end =
							if i + 1 < vec.len() { vec[i + 1] } else { start.saturating_add(1) };
						Just(TrackerOp::Remove { start, end })
					}),
					0..20,
				)
				.prop_shuffle()
			};
			removes_strategy.prop_map(move |removes| {
				let mut ops = inserts.clone();
				ops.extend(removes);
				ops
			})
		})
	}

	proptest! {
		#![proptest_config(ProptestConfig {
			cases: 1000, .. ProptestConfig::default()
		  })]
		#[test]
		fn prop_tracker_ops_hold_invariants(ops in tracker_ops_strategy()) {
			let mut tracker = CompactHeightTracker::new();
			let mut reference = BTreeSet::new();

			for op in &ops {
				match *op {
					TrackerOp::Insert(x) => {
						tracker.insert(x);
						reference.insert(x);
					}
					TrackerOp::Remove { start, end } => {
						tracker.remove(start..end);
						reference.retain(|&v| !(start <= v && v < end));
					}
				}

				let tracker_heights = tracker.get_all_heights();
				prop_assert_eq!(&tracker_heights, &reference, "Tracker heights do not match reference set");

				// No duplicate heights
				prop_assert_eq!(tracker_heights.len(), reference.len());

				// Ranges in elections are non-overlapping and sorted
				let mut last_end = None;
				for (&start, &end) in tracker.elections.iter() {
					if let Some(prev_end) = last_end {
						prop_assert!(prev_end <= start, "Ranges overlap or are out of order");
					}
					prop_assert!(start < end, "Range start must be less than end");
					last_end = Some(end);
				}

				// All heights in get_all_heights are covered by the ranges in elections
				let mut covered = BTreeSet::new();
				for (&start, &end) in tracker.elections.iter() {
					covered.extend(start..end);
				}
				prop_assert_eq!(tracker_heights, covered);
			}
		}
	}
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Default,
)]
pub enum SafeModeStatus {
	Enabled,
	#[default]
	Disabled,
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgressInner<ChainBlockNumber: SaturatingStep + PartialOrd> {
	Progress(ChainBlockNumber),
	Reorg(RangeInclusive<ChainBlockNumber>),
}
