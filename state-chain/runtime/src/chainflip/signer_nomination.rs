use crate::{Runtime, Validator};
use cf_traits::{Chainflip, EpochIndex, EpochInfo};
use frame_support::Hashable;
use nanorand::{Rng, WyRand};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

/// Tries to select `n` items randomly from the provided Vec.
///
/// If `n` is greater than the length of the Vec, returns `None`, otherwise
/// `Some` Vec of length `n`.
fn try_select_random_subset<T>(seed: u64, n: usize, mut things: Vec<T>) -> Option<Vec<T>> {
	if things.is_empty() || n > things.len() {
		return None
	}
	if n == things.len() {
		return Some(things)
	}

	WyRand::new_seed(seed).shuffle(&mut things);
	Some(things.into_iter().take(n).collect())
}

/// Select `Some` single item pseudo-randomly from the list using the given seed.
///
/// Returns `None` if the list is empty.
fn select_one<T>(seed: u64, things: Vec<T>) -> Option<T> {
	if things.is_empty() {
		None
	} else {
		let index = WyRand::new_seed(seed).generate_range(0..things.len());
		things.into_iter().nth(index)
	}
}

/// Takes something `Hashable` and hashes it to generate a `u64` seed value.
fn seed_from_hashable<H: Hashable>(value: H) -> u64 {
	let mut bytes = [0u8; 8];
	bytes.copy_from_slice(&value.twox_128()[0..8]);
	u64::from_be_bytes(bytes)
}

fn eligible_authorities(
	exclude_ids: &[<Runtime as Chainflip>::ValidatorId],
) -> Vec<<Runtime as Chainflip>::ValidatorId> {
	<Validator as EpochInfo>::current_authorities()
		.into_iter()
		.collect::<BTreeSet<_>>()
		.difference(&exclude_ids.iter().cloned().collect())
		.cloned()
		.collect()
}

/// Nominates pseudo-random signers based on the provided seed.
pub struct RandomSignerNomination;

impl cf_traits::SignerNomination for RandomSignerNomination {
	type SignerId = <Runtime as Chainflip>::ValidatorId;

	fn nomination_with_seed<H: Hashable>(
		seed: H,
		exclude_ids: &[Self::SignerId],
	) -> Option<Self::SignerId> {
		select_one(seed_from_hashable(seed), eligible_authorities(exclude_ids))
	}

	fn threshold_nomination_with_seed<H: Hashable>(
		seed: H,
		epoch_index: EpochIndex,
	) -> Option<Vec<Self::SignerId>> {
		try_select_random_subset(
			seed_from_hashable(seed),
			cf_utilities::success_threshold_from_share_count(
				<Validator as EpochInfo>::authority_count_at_epoch(epoch_index).unwrap_or_default(),
			) as usize,
			eligible_authorities(&[]),
		)
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeSet;

	use super::*;

	/// Generates a set of authorities with the SignerId = index + 1
	fn authority_set(len: usize) -> Vec<u64> {
		(0..len as u64).collect::<Vec<_>>()
	}

	#[test]
	fn test_select_one() {
		// Expect an authority in a set of 150 authorities.
		let a = select_one(seed_from_hashable(String::from("seed")), authority_set(150)).unwrap();
		// Expect a different value for different seed (collision is unlikely).
		let b = select_one(seed_from_hashable(String::from("seedy")), authority_set(150)).unwrap();
		assert_ne!(a, b);
		// If an empty set is provided, the result is `None`
		assert!(select_one::<u64>(seed_from_hashable(String::from("seed")), vec![],).is_none());
	}

	fn assert_selected_subset_is_valid<T: Clone + Ord>(seed: u64, threshold: usize, set: Vec<T>) {
		let source = BTreeSet::from_iter(set.clone());
		let result = BTreeSet::from_iter(try_select_random_subset(seed, threshold, set).unwrap());
		assert!(result.len() == threshold);
		assert!(source.is_superset(&result))
	}

	#[test]
	fn test_random_subset_selection() {
		for seed in 0..100 {
			assert_selected_subset_is_valid(seed, 0, (0..5).collect());
			assert_selected_subset_is_valid(seed, 1, (0..5).collect());
			assert_selected_subset_is_valid(seed, 2, (0..5).collect());
			assert_selected_subset_is_valid(seed, 3, (0..5).collect());
			assert_selected_subset_is_valid(seed, 4, (0..5).collect());
			assert_selected_subset_is_valid(seed, 5, (0..5).collect());
		}
	}

	#[test]
	fn test_subset_selection_is_none() {
		for seed in 0..100 {
			// empty set is invalid
			assert_eq!(None, try_select_random_subset::<u64>(seed, 0, vec![]));
			// threshold can't be larger than the set size
			assert_eq!(None, try_select_random_subset(seed, 6, (0..5).collect()));
		}
	}

	#[test]
	fn different_seed_different_set() {
		let set = (0..150).collect::<Vec<_>>();
		for seed in 0..100 {
			// Note: strictly speaking these don't have to be different but the chances of a
			// collision should be quite low.
			assert_ne!(
				BTreeSet::from_iter(try_select_random_subset(seed, 100, set.clone()).unwrap()),
				BTreeSet::from_iter(
					try_select_random_subset(seed + 100, 100, set.clone()).unwrap()
				),
			);
		}
	}

	#[test]
	fn same_seed_same_set() {
		let set = (0..150).collect::<Vec<_>>();
		for seed in 0..100 {
			assert_eq!(
				BTreeSet::from_iter(try_select_random_subset(seed, 100, set.clone()).unwrap()),
				BTreeSet::from_iter(try_select_random_subset(seed, 100, set.clone()).unwrap()),
			);
		}
	}
}
