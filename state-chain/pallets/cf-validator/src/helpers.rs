use crate::Vec;
use nanorand::{Rng, WyRand};
use sp_std::collections::btree_set::BTreeSet;

/// Selects the old nodes that should participate in the handover ceremony.
/// We want to select as many olds that are also in the new set as possible.
/// This reduces the number of peers, and therefore p2p messages required to complete
/// the handover ceremony. It also minimises the chance of a participating node being offline.
pub fn select_sharing_participants<ValidatorId: PartialEq + Eq + Clone + Ord>(
	old_authorities: BTreeSet<ValidatorId>,
	new_authorities: &BTreeSet<ValidatorId>,
	block_number: u64,
) -> BTreeSet<ValidatorId> {
	assert!(!old_authorities.is_empty() && !new_authorities.is_empty());

	fn shuffle<I: IntoIterator<Item = T>, T>(i: I, block_number: u64) -> Vec<T> {
		let mut things: Vec<_> = i.into_iter().collect();
		WyRand::new_seed(block_number).shuffle(&mut things);
		things
	}

	let success_threshold =
		cf_utilities::success_threshold_from_share_count(old_authorities.len() as u32) as usize;

	let both = old_authorities.intersection(new_authorities);
	let shuffled_both = shuffle(both, block_number);

	let old_not_in_new = old_authorities.difference(new_authorities);
	let shuffled_old_not_in_new = shuffle(old_not_in_new, block_number);

	shuffled_both
		.into_iter()
		.chain(shuffled_old_not_in_new)
		.take(success_threshold)
		.cloned()
		.collect()
}

#[cfg(test)]
mod select_sharing_participants_tests {
	use cf_utilities::assert_panics;

	use super::*;

	type ValidatorId = u32;

	#[test]
	fn test_empty_old_authorities() {
		let old_authorities = BTreeSet::<ValidatorId>::default();
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);

		assert_panics!(select_sharing_participants(old_authorities, &new_authorities, 1));
	}

	#[test]
	fn test_empty_new_authorities() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::default();

		assert_panics!(select_sharing_participants(old_authorities, &new_authorities, 1));
	}

	#[test]
	fn test_no_intersection() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([6, 7, 8, 9, 10]);

		let sharing_participants =
			select_sharing_participants(old_authorities, &new_authorities, 1);

		assert!(new_authorities.is_disjoint(&sharing_participants));
	}

	#[test]
	fn partial_intersection_prioritises_authorities_who_stay() {
		const INTERSECTING_SET: [u32; 3] = [3, 4, 5];
		let old_authorities =
			BTreeSet::<ValidatorId>::from_iter([1, 2].iter().chain(&INTERSECTING_SET).cloned());
		let new_authorities =
			BTreeSet::<ValidatorId>::from_iter(INTERSECTING_SET.iter().chain(&[6, 7]).cloned());

		let sharing_participants =
			select_sharing_participants(old_authorities, &new_authorities, 1);

		assert!(INTERSECTING_SET.iter().all(|x| sharing_participants.contains(x)));
	}

	#[test]
	fn full_intersection_gets_threshold_amount_from_old_set() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);

		assert_eq!(select_sharing_participants(old_authorities, &new_authorities, 1).len(), 4);
	}

	#[test]
	fn test_success_threshold_exceeded() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 9, 10]);

		let sharing_participants =
			select_sharing_participants(old_authorities, &new_authorities, 1);

		// All thew new authorities are shared. There should be another 2 from the old authorities.
		assert_eq!(sharing_participants.len(), 7);
		assert!(new_authorities.iter().all(|x| sharing_participants.contains(x)));
	}
}
