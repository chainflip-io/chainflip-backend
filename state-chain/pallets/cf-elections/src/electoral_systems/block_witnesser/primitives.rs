use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use core::{
	iter::Step,
	ops::{Range, RangeInclusive},
};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	cmp::max,
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	iter,
	vec::Vec,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

use crate::electoral_systems::{
	block_height_tracking::{ChainBlockHashOf, ChainBlockNumberOf, ChainProgress},
	state_machine::core::{defx, fst, Hook, Validate},
};

use super::state_machine::{BWElectionType, BWTypes};

defx! {
	#[codec(encode_bound(
		ChainBlockNumberOf<T::Chain>: Encode,
		ChainBlockHashOf<T::Chain>: Encode,
		T::BlockData: Encode,
		T::ElectionTrackerEventHook: Encode
	))]
	pub struct ElectionTracker2[T: BWTypes] {
		/// The lowest block we haven't seen yet. I.e., we have seen blocks below.
		pub seen_heights_below: ChainBlockNumberOf<T::Chain>,

		/// We always create elections until the next priority height, even if we
		/// are in safe mode.
		pub priority_elections_below: ChainBlockNumberOf<T::Chain>,

		/// Block hashes we got from the BHW.
		pub queued_elections: BTreeMap<ChainBlockNumberOf<T::Chain>, ChainBlockHashOf<T::Chain>>,

		/// Block heights which are queued but already past the safetymargin don't
		/// have associated hashes. We just store a list of block height ranges.
		pub queued_safe_elections: CompactHeightTracker<ChainBlockNumberOf<T::Chain>>,

		/// Hashes of elections currently ongoing
		pub ongoing: BTreeMap<ChainBlockNumberOf<T::Chain>, BWElectionType<T::Chain>>,

		/// Optimistic blocks
		pub optimistic_block_cache: BTreeMap<ChainBlockNumberOf<T::Chain>, OptimisticBlock<T>>,

		/// debug hook
		pub events: T::ElectionTrackerEventHook,

	}

	validate this (else ElectionTrackerError) {

		is_valid: true

		// TODO:
		// - there are no hashes for old elections
		//-  elections and safe elections are disjoined

	}
}

impl<T: BWTypes> ElectionTracker2<T> {
	pub fn update_safe_elections(
		&mut self,
		reason: UpdateSafeElectionsReason,
		f: impl Fn(&mut CompactHeightTracker<ChainBlockNumberOf<T::Chain>>),
	) {
		let old = self.queued_safe_elections.clone();
		f(&mut self.queued_safe_elections);
		let new = self.queued_safe_elections.clone();
		self.events.run(ElectionTrackerEvent::UpdateSafeElections { old, new, reason });
	}

	pub fn start_more_elections(&mut self, max_ongoing: usize, safemode: SafeModeStatus) {
		// In case of a reorg we still want to recreate elections for blocks which we had
		// elections for previously AND were touched by the reorg
		let start_all_below = match safemode {
			SafeModeStatus::Disabled => self.seen_heights_below.clone(),
			SafeModeStatus::Enabled => self.priority_elections_below.clone(),
		};

		use BWElectionType::*;

		// schedule at most `max_new_elections`
		let max_new_elections = max_ongoing.saturating_sub(self.ongoing.len());

		let safe_elections = self
			.queued_safe_elections
			.extract(max_new_elections)
			.into_iter()
			.map(|height| (height, SafeBlockHeight));

		let hash_elections = self
			.queued_elections
			.extract_if(|n, _| *n < start_all_below)
			.map(|(height, hash)| (height, ByHash(hash)));
		let opti_elections = iter::once((self.seen_heights_below, Optimistic));

		self.ongoing.extend(
			safe_elections
				.chain(hash_elections)
				.chain(opti_elections)
				.take(max_new_elections),
		);
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
		received: &BWElectionType<T::Chain>,
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
				self.events.run(ElectionTrackerEvent::ComparingBlocks {
					height,
					hash: received_hash.clone(),
					received: received.clone(),
					current: current.clone(),
				});

				match (received, &current) {
					// if we receive a result for the same election type as is currently open,
					// we close it
					(a, b) if a == b => Some(current),

					// if we get consensus for a by-hash election whose hash doesn't match with
					// the hash we have currently, we keep it open
					(ByHash(a), ByHash(b)) if a != b => None,

					// if we get an optimistic consensus for an election that is already by-hash,
					// we check whether the `received_hash` is the same as the hash we're currently
					// querying for. If it is, we accept the optimistic block as result for the
					// by-hash election. otherwise we keep the by-hash election open.
					(Optimistic, ByHash(current_hash)) =>
						if received_hash.as_ref() == Some(current_hash) {
							Some(current)
						} else {
							None
						},

					// If we get an optimistic consensus for an election that is already past
					// safety-margin we ignore it, it's safer to re-query by block height. This
					// should virtually never happen, only in case where the querying takes a *very*
					// long time.
					(Optimistic, SafeBlockHeight) => None,

					// If we get a by-hash consensus for an election that is already past
					// safety-margin, we ignore it. We've already deleted the hash for this
					// election from storage, so we can't check whether we got the correct
					// block. It's safer to re-query.
					(ByHash(_), SafeBlockHeight) => None,

					// All other cases should be impossible
					(_, _) => None,
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
				ByHash(_) | SafeBlockHeight => Some(received_data),
			})
	}

	/// This function schedules all elections up to `range.end()`
	pub fn schedule_range(
		&mut self,
		// range: RangeInclusive<ChainBlockNumberOf<T::Chain>>,
		// mut hashes: BTreeMap<ChainBlockNumberOf<T::Chain>, ChainBlockHashOf<T::Chain>>,
		progress: ChainProgress<T::Chain>,

		safety_margin: usize,
	) -> Vec<(ChainBlockNumberOf<T::Chain>, OptimisticBlock<T>)> {
		todo!()

		/*

		// Check whether there is a reorg concerning elections we have started previously.
		// If there is, we ensure that all ongoing or previously finished elections inside the reorg
		// range are going to be restarted once there is the capacity to do so.
		if let Some(next_election) = self.next_election() {
			if *range.start() < next_election {
				// we set this value such that even in case of a reorg we create elections for up to
				// this block
				self.priority_elections_below =
					max(next_election, self.priority_elections_below.clone());
			}
		}

		// if there are safe elections scheduled for the reorg range, unschedule them,
		// because it might be that we're gonna schedule them by-hash
		self.queued_safe_elections.remove(range.clone());

		// QUESTION: currently, the following check ensures that
		// the highest scheduled election never decreases. Do we want this?
		// It's difficult to imagine a situation where the highest block number
		// after a reorg is lower than it was previously, and also, even if, in that
		// case we simply keep the higher number that doesn't seem to be too much of a problem.
		self.seen_heights_below =
			max(self.seen_heights_below.clone(), range.end().clone().saturating_forward(1));

		// if there are elections ongoing for the block heights we received, we stop them
		self.ongoing.retain(|height, _| !hashes.contains_key(height));

		// if we have optimistic blocks for the hashes we received, we will return them
		let optimistic_blocks: BTreeMap<_, _> = if !is_reorg {
			if self.queued_elections.is_empty() {
				// we only want to use the single next block
				let next_queued_height = hashes.first_key_value().unwrap().0.clone();

				self.optimistic_block_cache
					.extract_if(|height, block| {
						hashes.get(height) == Some(&block.hash) && *height == next_queued_height
					})
					.collect()
			} else {
				Default::default()
			}
		} else {
			Default::default()
		};

		// remove those hashes for which we had optimistic blocks
		let _ = hashes
			.extract_if(|height, hash| {
				optimistic_blocks.get(&height).map(|block| block.hash == *hash).unwrap_or(false)
			})
			.collect::<Vec<_>>();

		// adding all hashes to the queue
		self.queued_elections.append(&mut hashes);

		// clean up the queue by removing old hashes
		let _ = self
			.queued_elections
			.extract_if(|height, _| height.saturating_forward(safety_margin) < *range.end())
			.map(fst)
			.for_each(|height| {
				self.queued_safe_elections.insert(height);
			});

		// move ongoing elections from ByHash to SafeBlockHeight if they become old enough
		self.ongoing.iter_mut().for_each(|(height, ty)| {
			if height.saturating_forward(safety_margin) < *range.end() {
				*ty = BWElectionType::SafeBlockHeight;
			}
		});

		optimistic_blocks.into_iter().collect()
		 */
	}

	fn next_election(&self) -> Option<ChainBlockNumberOf<T::Chain>> {
		self.queued_elections.first_key_value().map(fst).cloned()
	}
}

impl<T: BWTypes> Default for ElectionTracker2<T> {
	fn default() -> Self {
		Self {
			seen_heights_below: ChainBlockNumberOf::<T::Chain>::zero(),
			priority_elections_below: ChainBlockNumberOf::<T::Chain>::zero(),
			queued_elections: Default::default(),
			ongoing: Default::default(),
			queued_safe_elections: Default::default(),
			optimistic_block_cache: Default::default(),
			events: Default::default(),
		}
	}
}

#[derive_where(Debug, Clone, PartialEq, Eq;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	ChainBlockNumberOf<T::Chain>: Encode,
	ChainBlockHashOf<T::Chain>: Encode,
	T::BlockData: Encode,
))]
pub struct OptimisticBlock<T: BWTypes> {
	pub hash: ChainBlockHashOf<T::Chain>,
	pub data: T::BlockData,
}

impl<T: BWTypes> Validate for OptimisticBlock<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[derive_where(Debug, Clone, PartialEq, Eq;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ElectionTrackerEvent<T: BWTypes> {
	ComparingBlocks {
		height: ChainBlockNumberOf<T::Chain>,
		hash: Option<ChainBlockHashOf<T::Chain>>,
		received: BWElectionType<T::Chain>,
		current: BWElectionType<T::Chain>,
	},
	UpdateSafeElections {
		old: CompactHeightTracker<ChainBlockNumberOf<T::Chain>>,
		new: CompactHeightTracker<ChainBlockNumberOf<T::Chain>>,
		reason: UpdateSafeElectionsReason,
	},
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum UpdateSafeElectionsReason {
	OutOfSafetyMargin,
	SafeElectionScheduled,
	GotOptimisticBlock,
	ReorgReceived,
}

#[derive_where(Default; )]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct CompactHeightTracker<N> {
	elections: VecDeque<Range<N>>,
}

impl<N> Validate for CompactHeightTracker<N> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<N: Step + Ord> CompactHeightTracker<N> {
	pub fn extract(&mut self, max_elements: usize) -> Vec<N> {
		let mut result = Vec::new();
		for r in self.elections.iter_mut() {
			while result.len() < max_elements {
				if let Some(x) = r.next() {
					result.push(x);
				} else {
					// TODO: delete ranges when they are empty
					break;
				}
			}
		}
		result
	}

	pub fn insert(&mut self, item: N) {
		for r in self.elections.iter_mut().rev() {
			let end_plus_one = N::forward(r.end.clone(), 1);
			if item == end_plus_one {
				r.end = end_plus_one;
				return;
			}
			if r.contains(&item) {
				return;
			}
		}
		self.elections.push_back(item.clone()..N::forward(item, 1));
	}

	pub fn remove(&mut self, range: RangeInclusive<N>) {
		for r in self.elections.iter_mut() {
			range_difference(r, &(range.start().clone()..N::forward(range.end().clone(), 1)))
		}
	}
}

fn range_difference<N: Ord + Clone>(r: &mut Range<N>, s: &Range<N>) {
	// ```
	// [-- r --]
	//     [-- s --]
	// ```
	if s.contains(&r.end) {
		r.end = s.start.clone();
	}

	// ```
	//     [-- r --]
	// [-- s --]
	// ```
	if s.contains(&r.start) {
		r.start = s.end.clone();
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum SafeModeStatus {
	Enabled,
	Disabled,
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgressInner<ChainBlockNumber: SaturatingStep + PartialOrd> {
	Progress(ChainBlockNumber),
	Reorg(RangeInclusive<ChainBlockNumber>),
}
