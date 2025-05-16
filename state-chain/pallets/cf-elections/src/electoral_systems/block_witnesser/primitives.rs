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
	cmp::{max, min},
	collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque},
	iter,
	vec::Vec,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

use crate::electoral_systems::{
	block_height_tracking::ChainTypes,
	state_machine::core::{fst, Hook, Validate},
};

use super::state_machine::{BWElectionType, BWTypes};

macro_rules! do_match {
	($($tt:tt)*) => {
		|x| match x {$($tt)*}
	};
}

macro_rules! defx {
	(
		pub struct $Name:tt [$($ParamName:ident: $ParamType:tt),*] {
			$($Definition:tt)*
		} where $this:ident {
			$($prop_name:ident : $prop:expr),*
		} with {
			$($Attributes:tt)*
		}
	) => {

		$($Attributes)*
		pub struct $Name<$($ParamName: $ParamType),*> {
			$($Definition)*
		}

		impl<$($ParamName: $ParamType),*> Validate for $Name<$($ParamName),*> {

			type Error = &'static str;

			fn is_valid(&self) -> Result<(), Self::Error> {
				let $this = self;
				use frame_support::ensure;
				$(
					ensure!($prop, stringify!($prop_name));
				)*
				Ok(())
			}
		}
	};
}

defx! {

	pub struct ElectionTracker2[T: BWTypes] {
		/// The lowest block we haven't seen yet. I.e., we have seen blocks below.
		pub seen_heights_below: T::ChainBlockNumber,

		/// We always create elections until the next priority height, even if we
		/// are in safe mode.
		pub priority_elections_below: T::ChainBlockNumber,

		/// Block hashes we got from the BHW.
		pub queued_elections: BTreeMap<T::ChainBlockNumber, T::ChainBlockHash>,

		/// Block heights which are queued but already past the safetymargin don't
		/// have associated hashes. We just store the lowest height which we want
		/// to query.
		// pub queued_next_safe_height: Option<T::ChainBlockNumber>,

		pub queued_safe_elections: CompactHeightTracker<T::ChainBlockNumber>,

		/// Hashes of elections currently ongoing
		pub ongoing: BTreeMap<T::ChainBlockNumber, BWElectionType<T>>,

		/// Optimistic blocks
		pub optimistic_block_cache: BTreeMap<T::ChainBlockNumber, OptimisticBlock<T>>,

		/// debug hook
		pub events: T::ElectionTrackerEventHook,

	} where this {

		// queued_elections_are_consequtive:
		// 	this.queued_elections.keys().zip(this.queued_elections.keys().skip(1))
		// 	.all(|(left, right)| left.saturating_forward(1) == *right),

		// queued_safe_height_is_not_queued:
		// 	this.queued_safe_elections.clone().all(|height| !this.queued_elections.contains_key(&height))

	} with {

		#[derive_where(Debug, Clone, PartialEq, Eq;)]
		#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
		#[codec(encode_bound(
			T::ChainBlockNumber: Encode,
			T::ChainBlockHash: Encode,
			T::BlockData: Encode,
			T::ElectionTrackerEventHook: Encode
		))]

	}

}

impl<T: BWTypes> ElectionTracker2<T> {
	pub fn update_safe_elections(
		&mut self,
		reason: UpdateSafeElectionsReason,
		f: impl Fn(&mut CompactHeightTracker<T::ChainBlockNumber>),
	) {
		let old = self.queued_safe_elections.clone();
		f(&mut self.queued_safe_elections);
		let new = self.queued_safe_elections.clone();
		self.events.run(ElectionTrackerEvent::UpdateSafeElections { old, new, reason });
	}

	/// There are three types of elections:
	///  - ByHash() elections are started
	pub fn start_more_elections(&mut self, max_ongoing: usize, safemode: SafeModeStatus) {
		// In case of a reorg we still want to recreate elections for blocks which we had
		// elections for previously AND were touched by the reorg
		let start_all_below = match safemode {
			SafeModeStatus::Disabled => self.seen_heights_below.clone(),
			SafeModeStatus::Enabled => self.priority_elections_below.clone(),
		};

		use BWElectionType::*;

		// let safe_elections =
		// 	self.queued_safe_elections
		// 	.clone()
		// 	.map(|height| (height, SafeBlockHeight));

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
				// .inspect(|(height, election_type)| {
				// 	match *election_type {
				// 		Optimistic => (),
				// 		ByHash(_) =>
				// self.update_safe_elections(UpdateSafeElectionsReason::SafeElectionScheduled, |x|
				// *x = *height..*height), 		SafeBlockHeight =>
				// 			self.queued_safe_elections = self.queued_safe_elections.start..*height,
				// 	}
				// 	// if *election_type != Optimistic {
				// 	// 	self.queued_next_safe_height = Some(height.saturating_forward(1))
				// 	// }
				// })
				.take(max_new_elections),
		);

		let highest_non_optimistic_election = self
			.ongoing
			.iter()
			.filter(|(height, t)| **t != Optimistic)
			.map(fst)
			.max()
			.cloned();
		if let Some(n) = highest_non_optimistic_election {
			self.update_safe_elections(UpdateSafeElectionsReason::SafeElectionScheduled, |x| {
				// x.start = n.saturating_forward(1)
			});
		}
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
		height: T::ChainBlockNumber,
		received: &BWElectionType<T>,
		received_hash: &Option<T::ChainBlockHash>,
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
		range: RangeInclusive<T::ChainBlockNumber>,
		mut hashes: BTreeMap<T::ChainBlockNumber, T::ChainBlockHash>,
		safety_margin: usize,
		is_reorg: bool,
	) -> Vec<(T::ChainBlockNumber, OptimisticBlock<T>)> {
		// Check whether there is a reorg concerning elections we have started previously.
		// If there is, we ensure that all ongoing or previously finished elections inside the reorg
		// range are going to be restarted once there is the capacity to do so.
		if let Some(next_election) = self.next_election() {
			if *range.start() < next_election {
				// we set this value such that even in case of a reorg we create elections for up to
				// this block
				self.priority_elections_below =
					max(next_election, self.priority_elections_below.clone());

				// We have to kick out scheduled safe-height elections that intersect with the reorg
				// we're gonna create by-hash elections for the reorg range instead
				// self.queued_next_safe_height = self.queued_next_safe_height.map(
				// 	|next_safe_height| min(next_safe_height, *range.start())
				// );
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
			.map(fst)
			.min()
			.inspect(|height| {
				self.update_safe_elections(UpdateSafeElectionsReason::GotOptimisticBlock, |x|
					// x.start = max(x.start, *height));

					// NOTE: it seems like here we shouldn't do anything, remove the whole call if things work out
					())
			});

		if is_reorg {
			self.update_safe_elections(UpdateSafeElectionsReason::ReorgReceived, |x|

				// NOTE, we shouldnt have to do this. This is scheduling new elections for blocks inside the safety margin,
				// that were reorged
				// x.start = min(x.start, *range.start()));
				());
		}

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

		// .max()
		// .clone()
		// .inspect(|height| {
		// 	self.update_safe_elections(UpdateSafeElectionsReason::OutOfSafetyMargin, |x| {
		// 		x.end = max(x.end, height.saturating_forward(1))
		// 	});
		// });

		optimistic_blocks.into_iter().collect()
	}

	fn next_election(&self) -> Option<T::ChainBlockNumber> {
		self.queued_elections.first_key_value().map(fst).cloned()
		// .unwrap_or(self.seen_heights_below.clone())
	}
}

impl<T: BWTypes> Default for ElectionTracker2<T> {
	fn default() -> Self {
		Self {
			seen_heights_below: T::ChainBlockNumber::zero(),
			priority_elections_below: T::ChainBlockNumber::zero(),
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
	T::ChainBlockNumber: Encode,
	T::ChainBlockHash: Encode,
	T::BlockData: Encode,
))]
pub struct OptimisticBlock<T: BWTypes> {
	pub hash: T::ChainBlockHash,
	pub data: T::BlockData,
}

#[derive_where(Debug, Clone, PartialEq, Eq;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ElectionTrackerEvent<T: BWTypes> {
	ComparingBlocks {
		height: T::ChainBlockNumber,
		hash: Option<T::ChainBlockHash>,
		received: BWElectionType<T>,
		current: BWElectionType<T>,
	},
	UpdateSafeElections {
		old: CompactHeightTracker<T::ChainBlockNumber>,
		new: CompactHeightTracker<T::ChainBlockNumber>,
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

/// Keeps track of ongoing elections for the block witnesser.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct ElectionTracker<N: Ord> {
	/// The highest block height for which an election was started in the past.
	/// New elections are going to be started if there is the capacity to do so
	/// and that height has been witnessed (`highest_witnessed > highest_election`).
	pub next_election: N,

	/// The highest block height that has been seen.
	pub next_witnessed: N,

	/// The highest block height that we had previously started elections for and
	/// that was subsequently touched by a reorg.
	pub next_priority_election: N,

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
			SafeModeStatus::Disabled => self.next_witnessed,
			SafeModeStatus::Enabled => self.next_priority_election,
		};

		// filter out all elections which are ongoing, but shouldn't be, because
		// they are in the scheduled range (for example because there was a reorg)
		self.ongoing.retain(|height, _| *height < self.next_election);

		// schedule
		for height in self.next_election..start_up_to {
			if self.ongoing.len() < max_ongoing {
				self.ongoing.insert(height, self.reorg_id);
				self.next_election = height.saturating_forward(1);
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
		if *range.start() < self.next_election {
			// we set this value such that even in case of a reorg we create elections for up to
			// this block
			self.next_priority_election = max(self.next_election, self.next_priority_election);

			// the next election we start is going to be the first block involved in the reorg
			self.next_election = *range.start();

			// and it's going to have a fresh `reorg_id` which forces the ES to recreate this
			// election
			self.reorg_id =
				generate_new_reorg_id(&self.ongoing.values().cloned().collect::<Vec<_>>());
		}

		// QUESTION: currently, the following check ensures that
		// the highest scheduled election never decreases. Do we want this?
		// It's difficult to imagine a situation where the highest block number
		// after a reorg is lower than it was previously, and also, even if, in that
		// case we simply keep the higher number that doesn't seem to be too much of a problem.
		if self.next_witnessed <= *range.end() {
			self.next_witnessed = range.end().saturating_forward(1);
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
			next_witnessed: BlockZero::zero(),
			next_priority_election: BlockZero::zero(),
			next_election: BlockZero::zero(),
			ongoing: Default::default(),
			reorg_id: 0,
		}
	}
}

/// Generates an element which is not in `indices`.
fn generate_new_reorg_id<N: BlockZero + SaturatingStep + Ord + 'static>(indices: &[N]) -> N {
	let mut index = N::zero();
	while indices.iter().any(|ix| *ix == index) {
		index = index.saturating_forward(1);
	}
	index
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgressInner<ChainBlockNumber: SaturatingStep + PartialOrd> {
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
			assert!(!xs.contains(&generate_new_reorg_id(&xs)));
		}
	}
}
