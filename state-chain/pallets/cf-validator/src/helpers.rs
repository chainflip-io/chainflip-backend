// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::Vec;
use nanorand::{Rng, WyRand};
use sp_std::collections::btree_set::BTreeSet;

/// Selects the old nodes that should participate in the handover ceremony.
/// We want to select as many olds that are also in the new set as possible.
/// This reduces the number of peers, and therefore p2p messages required to complete
/// the handover ceremony. It also minimises the chance of a participating node being offline.
///
/// If no sharing set can be determined, returns None.
pub fn select_sharing_participants<
	ValidatorId: sp_std::fmt::Debug + PartialEq + Eq + Clone + Ord,
>(
	success_threshold: u32,
	current_authorities: BTreeSet<ValidatorId>,
	new_authorities: &BTreeSet<ValidatorId>,
	block_number: u64,
) -> Option<BTreeSet<ValidatorId>> {
	fn shuffle<I: IntoIterator<Item = T>, T>(i: I, block_number: u64) -> Vec<T> {
		let mut things: Vec<_> = i.into_iter().collect();
		WyRand::new_seed(block_number).shuffle(&mut things);
		things
	}

	if (current_authorities.len() as u32) < success_threshold || new_authorities.is_empty() {
		return None
	}

	let both = current_authorities.intersection(new_authorities);
	let shuffled_both = shuffle(both, block_number);

	let old_not_in_new = current_authorities.difference(new_authorities);
	let shuffled_old_not_in_new = shuffle(old_not_in_new, block_number);

	Some(
		shuffled_both
			.into_iter()
			.chain(shuffled_old_not_in_new)
			.take(success_threshold as usize)
			.cloned()
			.collect(),
	)
}

#[cfg(test)]
mod select_sharing_participants_tests {
	use cf_utilities::success_threshold_from_share_count;

	use super::*;

	type ValidatorId = u32;

	#[test]
	fn test_empty_old_authorities() {
		let old_authorities = BTreeSet::<ValidatorId>::default();
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);

		assert!(select_sharing_participants(3, old_authorities, &new_authorities, 1).is_none());
	}

	#[test]
	fn test_empty_new_authorities() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::default();

		assert!(select_sharing_participants(1, old_authorities, &new_authorities, 1).is_none());
	}

	#[test]
	fn test_no_intersection() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([6, 7, 8, 9, 10]);

		let threshold = success_threshold_from_share_count(old_authorities.len() as u32);

		let sharing_participants =
			select_sharing_participants(threshold, old_authorities, &new_authorities, 1).unwrap();

		assert!(new_authorities.is_disjoint(&sharing_participants));
	}

	#[test]
	fn partial_intersection_prioritises_authorities_who_stay() {
		let intersecting_set = BTreeSet::<ValidatorId>::from_iter([3, 4, 5]);

		let old_authorities: BTreeSet<_> =
			intersecting_set.union(&BTreeSet::from_iter([1, 2])).copied().collect();

		let new_authorities: BTreeSet<_> =
			intersecting_set.union(&BTreeSet::from_iter([6, 7])).copied().collect();

		let threshold =
			cf_utilities::success_threshold_from_share_count(old_authorities.len() as u32);

		let sharing_participants =
			select_sharing_participants(threshold, old_authorities.clone(), &new_authorities, 1)
				.unwrap();

		assert!(intersecting_set.is_subset(&sharing_participants));
		assert_eq!(sharing_participants.len(), threshold as usize);
		assert!(sharing_participants.is_subset(&old_authorities));
	}

	#[test]
	fn full_intersection_gets_threshold_amount_from_old_set() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5]);

		let threshold =
			cf_utilities::success_threshold_from_share_count(old_authorities.len() as u32);

		assert_eq!(
			select_sharing_participants(threshold, old_authorities, &new_authorities, 1)
				.unwrap()
				.len(),
			4
		);
	}

	#[test]
	fn test_success_threshold_exceeded() {
		let old_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 9, 10]);

		let threshold = success_threshold_from_share_count(old_authorities.len() as u32);

		let sharing_participants =
			select_sharing_participants(threshold, old_authorities, &new_authorities, 1).unwrap();

		// All thew new authorities are shared. There should be another 2 from the old authorities.
		assert_eq!(sharing_participants.len(), threshold as usize);
		assert!(new_authorities.is_subset(&sharing_participants))
	}

	#[test]
	fn test_none_if_old_authority_threshold_not_met() {
		let new_authorities = BTreeSet::<ValidatorId>::from([1, 2, 3, 9, 10]);
		const THRESHOLD: u32 = 5;

		assert!(select_sharing_participants(
			THRESHOLD,
			BTreeSet::<ValidatorId>::from_iter(0..THRESHOLD - 1),
			&new_authorities,
			1
		)
		.is_none());
		assert!(select_sharing_participants(
			THRESHOLD,
			BTreeSet::<ValidatorId>::from_iter(0..THRESHOLD),
			&new_authorities,
			1
		)
		.is_some());
	}

	#[test]
	fn selection_varies_with_the_block_number() {
		// When a handover fails without any node being blamed, nothing is banned
		// and the only thing that changes between attempts is the block number.
		// Re-shuffling on every attempt is therefore the sole mechanism by which
		// retries can make progress, so it needs to actually vary the selection.
		let current_authorities = BTreeSet::<ValidatorId>::from_iter(0..20);
		let new_authorities = BTreeSet::<ValidatorId>::from_iter(20..40);
		let threshold = success_threshold_from_share_count(current_authorities.len() as u32);

		let selections = (1..10u64)
			.map(|block_number| {
				select_sharing_participants(
					threshold,
					current_authorities.clone(),
					&new_authorities,
					block_number,
				)
				.unwrap()
			})
			.collect::<BTreeSet<_>>();

		assert!(selections.len() > 1, "selection did not vary across blocks");
	}

	proptest::proptest! {
		#[test]
		fn sharing_set_is_always_a_valid_reconstruction_set(
			// The full authority set: this is what determines how many key shares
			// are needed, regardless of how many of them have been banned.
			current_authorities in proptest::collection::btree_set(0u32..40, 1..40),
			banned_indices in proptest::collection::btree_set(0usize..40, 0..40),
			new_authorities in proptest::collection::btree_set(20u32..60, 0..40),
			block_number in 0u64..1000,
		) {
			let threshold = success_threshold_from_share_count(current_authorities.len() as u32);

			// Banning never changes the threshold, only the pool to select from.
			let unbanned = current_authorities
				.iter()
				.enumerate()
				.filter_map(|(i, id)| (!banned_indices.contains(&i)).then_some(*id))
				.collect::<BTreeSet<_>>();

			let selected = select_sharing_participants(
				threshold,
				unbanned.clone(),
				&new_authorities,
				block_number,
			);

			if unbanned.len() < threshold as usize || new_authorities.is_empty() {
				proptest::prop_assert!(selected.is_none());
			} else {
				let selected = selected.unwrap();

				// Enough key holders to reconstruct the key, and not one more than
				// necessary: every extra participant is another single point of
				// failure for the ceremony.
				proptest::prop_assert_eq!(selected.len(), threshold as usize);

				// Only unbanned current authorities hold a share of the key.
				proptest::prop_assert!(selected.is_subset(&unbanned));

				// Authorities who are staying on are preferred, to minimise both the
				// number of p2p peers and the chance of picking a node that is on its
				// way out.
				let staying = unbanned.intersection(&new_authorities).count();
				proptest::prop_assert_eq!(
					selected.intersection(&new_authorities).count(),
					core::cmp::min(staying, threshold as usize)
				);
			}
		}
	}
}
