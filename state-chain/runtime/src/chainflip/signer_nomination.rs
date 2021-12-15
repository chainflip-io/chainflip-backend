use crate::{Online, Runtime};
use cf_traits::{Chainflip, IsOnline};
use frame_support::{Hashable, storage::IterableStorageMap};
use nanorand::{Rng, WyRand};
use sp_std::vec::Vec;
use codec::{Encode, Decode};

// /// Returns a scaled index based on an input seed
// fn get_random_index(seed: Vec<u8>, max: usize) -> usize {
// 	let hash = twox_128(&seed);
// 	let index = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]) % max as u32;
// 	index as usize
// }

// /// Select the next signer
// fn select_signer<SignerId: Clone, T: IsOnline<ValidatorId = SignerId>>(
// 	validators: Vec<(SignerId, ())>,
// 	seed: Vec<u8>,
// ) -> Option<SignerId> {
// 	// Get all online validators
// 	let online_validators =
// 		validators.iter().filter(|(id, _)| T::is_online(id)).collect::<Vec<_>>();
// 	let number_of_online_validators = online_validators.len();
// 	// Check if there is someone online
// 	if number_of_online_validators == 0 {
// 		return None
// 	}
// 	// Get a a pseudo random id by which we choose the next validator
// 	let the_chosen_one = get_random_index(seed, number_of_online_validators);
// 	online_validators.get(the_chosen_one).map(|f| f.0.clone())
// }

/// Select a random subset of size `n` from the set of `things`.
///
/// Returns `None` if `n` is larger than the number of things.
fn select_random_subset<T>(seed: u64, n: usize, mut things: Vec<T>) -> Option<Vec<T>> {
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

fn get_online_validators(
) -> Vec<<Runtime as Chainflip>::ValidatorId> {
	pallet_cf_validator::ValidatorLookup::<Runtime>::iter_keys()
		.filter(|id| <Online as cf_traits::IsOnline>::is_online(&id))
		.collect()
}

impl cf_traits::SignerNomination for RandomSignerNomination {
	type SignerId = <Runtime as Chainflip>::ValidatorId;

	fn nomination_with_seed<H: Hashable>(seed: H) -> Option<Self::SignerId> {
		let online_validators = get_online_validators();
		select_one(seed_from_hashable(seed), online_validators)
	}

	fn threshold_nomination_with_seed<H: Hashable>(seed: H) -> Option<Vec<Self::SignerId>> {
		// TODO: get this from `EpochInfo` instead.
		let threshold = pallet_cf_witnesser::ConsensusThreshold::<Runtime>::get();
		let mut online_validators = get_online_validators();
		select_random_subset(
			seed_from_hashable(seed),
			threshold as usize,
			online_validators,
		)
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeSet;

	use super::*;
	use cf_traits::IsOnline;
	use frame_support::traits::InstanceFilter;
	use sp_std::cell::RefCell;
	// use std::ops::Range;

	/// Generates a set of validators with the SignerId = index + 1
	fn validator_set(len: usize) -> Vec<u64> {
		(0..len as u64).collect::<Vec<_>>()
	}

	// thread_local! {
	// 	// Switch to control the mock
	// 	pub static ONLINE: RefCell<bool>  = RefCell::new(true);
	// }

	// struct MockIsOnline;
	// impl IsOnline for MockIsOnline {
	// 	type ValidatorId = u64;

	// 	fn is_online(_validator_id: &Self::ValidatorId) -> bool {
	// 		ONLINE.with(|cell| cell.borrow().clone())
	// 	}
	// }

	// #[test]
	// fn test_get_random_index() {
	// 	assert!(get_random_index(vec![1, 6, 7, 4, 6, 7, 8], 5) < 5);
	// 	assert!(get_random_index(vec![0, 0, 0], 5) < 5);
	// 	assert!(get_random_index(vec![180, 200, 240], 10) < 10);
	// }

	#[test]
	fn test_select_signer() {
		// Expect Some validator
		assert!(
			select_one(
				seed_from_hashable(vec![2, 5, 7, 3]),
				vec![(4, ()), (6, ()), (7, ()), (9, ())],
			).is_some()
		);
		// Expect a validator in a set of 150 validators
		assert!(
			select_one(
				seed_from_hashable(String::from("seed").into_bytes()),
				validator_set(150),
			).is_some()
		);
		// Expect an comparable big change in the value
		// distribution for an small input seed change
		assert!(
			select_one(
				seed_from_hashable(("seedy", "seed")),
				validator_set(150),
			).is_some()
		);
		// Expect an reasonable SignerId for an bigger input seed
		assert!(
			select_one(
				seed_from_hashable(("west1_north_south_east:_berlin_zonk", 1, 2, 3, 4u128))
				validator_set(150),
			).is_some()
		);
		// Expect the select_signer function to return None
		// if there is currently no online validator
		assert!(
			select_one(
				seed_from_hashable("seed")
				vec![],
			).is_none()
		);
	}

	fn test_subset_with<T>(seed: u64, threshold: u64, set: Vec<T>) {
		let source = BTreeSet::from_iter(set.clone());
		let result = BTreeSet::from_iter(select_random_subset(SEED, threshold, set));
		assert!(result.len() == threshold);
		assert!(source.is_superset(result))
	}

	#[test]
	fn test_random_subset_selection() {
		const SEED: u64 = 0;
		test_subset_with(SEED, 2, (0..5).collect());
		test_subset_with(SEED, 3, (0..5).collect());
		test_subset_with(SEED, 4, (0..5).collect());
		test_subset_with(SEED, 5, (0..5).collect());
	}
}
