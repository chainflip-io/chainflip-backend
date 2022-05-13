use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use sp_runtime::traits::{AtLeast32BitUnsigned, BlockNumberProvider};
use sp_std::{
	collections::{btree_set::BTreeSet, vec_deque::VecDeque},
	iter,
};

#[derive(Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode)]
pub struct SuspensionTracker<Id, Block, Offence> {
	offence: Offence,
	current_block: Block,
	all: VecDeque<(Block, Id)>,
}

pub trait StorageLoadable<T> {
	type StorageKey: Encode + Decode;
	type StoredAs: Encode + Decode;

	fn load(key: &Self::StorageKey) -> Self;
	fn commit(&mut self);
}

impl<T: crate::Config> StorageLoadable<T>
	for SuspensionTracker<T::ValidatorId, T::BlockNumber, T::Offence>
{
	type StorageKey = T::Offence;
	type StoredAs = VecDeque<(T::BlockNumber, T::ValidatorId)>;

	fn load(key: &Self::StorageKey) -> Self {
		Self {
			offence: *key,
			current_block: frame_system::Pallet::<T>::current_block_number(),
			all: crate::Suspensions::<T>::get(key),
		}
	}

	fn commit(&mut self) {
		self.release_expired();
		crate::Suspensions::<T>::insert(self.offence, self.all.clone());
	}
}

impl<Id, Block, Offence> SuspensionTracker<Id, Block, Offence>
where
	Block: AtLeast32BitUnsigned + Copy,
	Id: Ord + Clone,
{
	/// Suspend a list of nodes for a number of blocks.
	pub fn suspend(&mut self, ids: impl IntoIterator<Item = Id>, duration: Block) {
		let current_block = self.current_block;
		self.all
			.extend(iter::repeat_with(move || current_block.saturating_add(duration)).zip(ids));
		self.all.make_contiguous().sort_unstable_by_key(|(block, _)| *block);
	}

	/// Release any nodes whose suspension period has expired.
	pub fn release_expired(&mut self) {
		while matches!(self.all.front(), Some((block, _)) if *block < self.current_block) {
			self.all.pop_front();
		}
	}

	/// Get the set of currently suspended validators.
	pub fn get_suspended(&self) -> BTreeSet<Id> {
		self.all
			.iter()
			.skip_while(move |(block, _)| *block < self.current_block)
			.map(|(_, id)| id)
			.cloned()
			.collect()
	}
}

#[cfg(test)]
mod test_suspension_tracking {
	use sp_std::iter::FromIterator;

	use super::*;

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
	enum Offence {
		EatingTheLastRolo,
	}

	type TestSuspensionTracker = SuspensionTracker<u32, u32, Offence>;

	impl TestSuspensionTracker {
		pub fn advance_blocks(&mut self, blocks: u32) {
			self.current_block += blocks;
		}
	}

	#[test]
	fn test_tracker() {
		const SUSPENSION_DURATION: u32 = 10;

		let mut tracker = TestSuspensionTracker {
			offence: Offence::EatingTheLastRolo,
			current_block: 0,
			all: Default::default(),
		};

		tracker.suspend([1, 2, 3], SUSPENSION_DURATION);

		assert_eq!(tracker.get_suspended(), BTreeSet::from_iter([1, 2, 3]), "{:?}", tracker);

		tracker.advance_blocks(1);
		tracker.suspend([3, 4, 5], SUSPENSION_DURATION);

		assert_eq!(tracker.get_suspended(), BTreeSet::from_iter([1, 2, 3, 4, 5]), "{:?}", tracker);

		tracker.advance_blocks(SUSPENSION_DURATION);

		assert_eq!(tracker.get_suspended(), BTreeSet::from_iter([3, 4, 5]), "{:?}", tracker);
		assert_eq!(
			tracker.all.iter().map(|(_, id)| *id).collect::<BTreeSet<_>>(),
			[1, 2, 3, 4, 5].into_iter().collect::<BTreeSet<_>>()
		);
		tracker.release_expired();
		assert_eq!(
			tracker.all.iter().map(|(_, id)| *id).collect::<BTreeSet<_>>(),
			[3, 4, 5].into_iter().collect::<BTreeSet<_>>()
		);

		tracker.advance_blocks(1);

		assert_eq!(tracker.get_suspended(), BTreeSet::from_iter([]), "{:?}", tracker);

		assert!(!tracker.all.is_empty());
		tracker.release_expired();
		assert!(tracker.all.is_empty());
	}
}
