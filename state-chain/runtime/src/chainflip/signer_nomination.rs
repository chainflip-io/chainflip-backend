use crate::{Online, Runtime, Validator};
use cf_traits::{Chainflip, EpochInfo};
use frame_support::Hashable;
use nanorand::{Rng, WyRand};
use sp_std::vec::Vec;

fn try_select_random_subset<T>(seed: u64, n: usize, mut things: Vec<T>) -> Option<Vec<T>> {
	if n > things.len() {
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
	if things.len() == 0 {
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

/// Nominates pseudo-random signers based on the provided seed.
pub struct RandomSignerNomination;

/// Returns a list of online validators.
///
/// TODO: When #1037 is merged, use the more efficient EpochInfo::current_validators()
fn get_online_validators() -> Vec<<Runtime as Chainflip>::ValidatorId> {
	pallet_cf_validator::ValidatorLookup::<Runtime>::iter()
		.filter_map(|(id, _)| {
			if <Online as cf_traits::IsOnline>::is_online(&id) {
				Some(id.clone())
			} else {
				None
			}
		})
		.collect()
}

impl cf_traits::SignerNomination for RandomSignerNomination {
	type SignerId = <Runtime as Chainflip>::ValidatorId;

	fn nomination_with_seed<H: Hashable>(seed: H) -> Option<Self::SignerId> {
		let online_validators = get_online_validators();
		select_one(seed_from_hashable(seed), online_validators)
	}

	fn threshold_nomination_with_seed<H: Hashable>(seed: H) -> Option<Vec<Self::SignerId>> {
		let threshold = <Validator as EpochInfo>::consensus_threshold();
		let online_validators = get_online_validators();
		try_select_random_subset(seed_from_hashable(seed), threshold as usize, online_validators)
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::BTreeSet, iter::FromIterator};

	use super::*;

	const SEED: u64 = 0;

	/// Generates a set of validators with the SignerId = index + 1
	fn validator_set(len: usize) -> Vec<u64> {
		(0..len as u64).collect::<Vec<_>>()
	}

	#[test]
	fn test_select_signer() {
		// Expect Some validator
		assert!(select_one(
			seed_from_hashable(vec![2, 5, 7, 3]),
			vec![(4, ()), (6, ()), (7, ()), (9, ())],
		)
		.is_some());
		// Expect a validator in a set of 150 validators
		assert!(select_one(
			seed_from_hashable(String::from(String::from("seed")).into_bytes()),
			validator_set(150),
		)
		.is_some());
		// Expect an comparable big change in the value
		// distribution for an small input seed change
		assert!(select_one(
			seed_from_hashable((String::from("seedy"), String::from("seed"))),
			validator_set(150),
		)
		.is_some());
		// Expect an reasonable SignerId for an bigger input seed
		assert!(select_one(
			seed_from_hashable((
				String::from("west1_north_south_east:_berlin_zonk"),
				1,
				2,
				3,
				4u128
			)),
			validator_set(150),
		)
		.is_some());
		// Expect the select_signer function to return None
		// if there is currently no online validator
		assert!(select_one::<u64>(seed_from_hashable(String::from("seed")), vec![],).is_none());
	}

	fn test_subset_with<T: Clone + Ord>(seed: u64, threshold: usize, set: Vec<T>) {
		let source = BTreeSet::from_iter(set.clone());
		let result = BTreeSet::from_iter(try_select_random_subset(seed, threshold, set).unwrap());
		assert!(result.len() == threshold);
		assert!(source.is_superset(&result))
	}

	#[test]
	fn test_random_subset_selection() {
		test_subset_with(SEED, 2, (0..5).collect());
		test_subset_with(SEED, 3, (0..5).collect());
		test_subset_with(SEED, 4, (0..5).collect());
		test_subset_with(SEED, 5, (0..5).collect());
	}

	#[test]
	fn different_seed_different_set() {
		let seed = 1;
		let set = (0..5).collect::<Vec<_>>();
		let b = BTreeSet::from_iter(try_select_random_subset(seed, 2, set.clone()).unwrap());
		let a = BTreeSet::from_iter(try_select_random_subset(seed + 1, 2, set.clone()).unwrap());

		assert_ne!(a, b);
	}
}
