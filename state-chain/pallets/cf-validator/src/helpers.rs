use sp_std::collections::btree_set::BTreeSet;

/// Selects the old nodes that should participate in the handover ceremony.
/// We want to select as many olds that are also in the new set as possible.
/// This reduces the number of peers, and therefore p2p messages required to complete
/// the handover ceremony. It also minimises the chance of a participating node being offline.
pub fn select_sharing_participants<ValidatorId: PartialEq + Eq + Clone + Ord>(
	old_authorities: BTreeSet<ValidatorId>,
	new_authorities: &BTreeSet<ValidatorId>,
) -> BTreeSet<ValidatorId> {
	assert!(!old_authorities.is_empty() && !new_authorities.is_empty());

	let success_threshold =
		cf_utilities::success_threshold_from_share_count(old_authorities.len() as u32) as usize;

	// Get the intersection of the old and new set.
	let both: BTreeSet<_> = old_authorities.intersection(new_authorities).cloned().collect();

	let n_both = both.len();
	if n_both >= success_threshold {
		both.into_iter().take(success_threshold).collect()
	} else {
		let both_lookup = both.clone();
		old_authorities
			.iter()
			.filter(|old_authority| !both_lookup.contains(old_authority))
			.take(success_threshold - n_both)
			.cloned()
			.chain(both)
			.collect()
	}
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

		assert_panics!(select_sharing_participants(old_authorities, &new_authorities));
	}

	#[test]
	fn test_empty_new_authorities() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::default();

		assert_panics!(select_sharing_participants(old_authorities, &new_authorities));
	}

	#[test]
	fn test_no_intersection() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([6, 7, 8, 9, 10]);
		assert_eq!(
			select_sharing_participants(old_authorities, &new_authorities),
			BTreeSet::from([1, 2, 3, 4])
		);
	}

	#[test]
	fn test_partial_intersection() {
		let old_authorities = BTreeSet::<ValidatorId>::from([4, 1, 3, 2, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([3, 4, 5, 6, 7]);

		// 1 and 2 are not in the new authorities, so only ordering determines that 1 is selected
		// over 2.
		assert_eq!(
			select_sharing_participants(old_authorities, &new_authorities),
			BTreeSet::from([1, 3, 4, 5])
		);
	}

	#[test]
	fn test_full_intersection() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);

		// Will just get a threshold amount of the old_authorities.
		assert_eq!(
			select_sharing_participants(old_authorities, &new_authorities),
			BTreeSet::from([1, 2, 3, 4])
		);
	}

	#[test]
	fn test_success_threshold_exceeded() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 9, 10]);

		// 1, 2, 3, 9, 10 are shared. 4 and 5 are the first non shared from the old set.
		assert_eq!(
			select_sharing_participants(old_authorities, &new_authorities),
			BTreeSet::from([1, 2, 3, 4, 5, 9, 10])
		);
	}
}
