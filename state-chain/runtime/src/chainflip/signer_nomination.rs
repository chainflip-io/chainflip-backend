use crate::{Reputation, Runtime, Validator};
use cf_primitives::EpochIndex;
use cf_traits::{Chainflip, EpochInfo};
use frame_support::Hashable;
use nanorand::{Rng, WyRand};
use pallet_cf_validator::HistoricalAuthorities;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

use super::Offence;

/// Tries to select `n` items randomly from the provided BTreeSet.
///
/// If `n` is greater than the length of the BTreeSet, returns `None`, otherwise
/// `Some` BTreeSet of length `n`.
fn try_select_random_subset<T>(seed: u64, n: usize, things: BTreeSet<T>) -> Option<BTreeSet<T>>
where
	T: Ord,
{
	if things.is_empty() || n > things.len() {
		return None
	}
	if n == things.len() {
		return Some(things)
	}

	let mut things: Vec<T> = things.into_iter().collect();
	WyRand::new_seed(seed).shuffle(&mut things);
	Some(things.into_iter().take(n).collect())
}

/// Select `Some` single item pseudo-randomly from the list using the given seed.
///
/// Returns `None` if the list is empty.
fn select_one<T>(seed: u64, things: BTreeSet<T>) -> Option<T> {
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
	at_epoch: EpochIndex,
	exclude_ids: &BTreeSet<<Runtime as Chainflip>::ValidatorId>,
) -> BTreeSet<<Runtime as Chainflip>::ValidatorId> {
	HistoricalAuthorities::<Runtime>::get(at_epoch)
		.into_iter()
		.collect::<BTreeSet<_>>()
		.difference(exclude_ids)
		.cloned()
		.collect()
}

/// Nominates pseudo-random signers based on the provided seed.
///
/// Signers serving a suspension for any of the offences in ExclusionOffences are
/// excluded from being nominated.
pub struct RandomSignerNomination;

impl cf_traits::BroadcastNomination for RandomSignerNomination {
	type BroadcasterId = <Runtime as Chainflip>::ValidatorId;

	fn nominate_broadcaster<H: Hashable>(
		seed: H,
		exclude_ids: impl IntoIterator<Item = Self::BroadcasterId>,
	) -> Option<Self::BroadcasterId> {
		let mut all_excludes = Reputation::validators_suspended_for(&[
			Offence::FailedToBroadcastTransaction,
			Offence::MissedHeartbeat,
		]);
		all_excludes.extend(exclude_ids);
		select_one(
			seed_from_hashable(seed),
			eligible_authorities(Validator::epoch_index(), &all_excludes),
		)
	}
}

impl cf_traits::ThresholdSignerNomination for RandomSignerNomination {
	type SignerId = <Runtime as Chainflip>::ValidatorId;

	fn threshold_nomination_with_seed<H: Hashable>(
		seed: H,
		epoch_index: EpochIndex,
	) -> Option<BTreeSet<Self::SignerId>> {
		try_select_random_subset(
			seed_from_hashable(seed),
			cf_utilities::success_threshold_from_share_count(Validator::authority_count_at_epoch(
				epoch_index,
			)?) as usize,
			eligible_authorities(
				epoch_index,
				&Reputation::validators_suspended_for(&[
					Offence::MissedHeartbeat,
					Offence::ParticipateSigningFailed,
					Offence::ParticipateKeygenFailed,
					Offence::ParticipateKeyHandoverFailed,
				]),
			),
		)
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use std::collections::BTreeSet;

	/// Generates a set of authorities with the SignerId = index + 1
	fn authority_set(len: usize) -> BTreeSet<u64> {
		(0..len as u64).collect::<BTreeSet<_>>()
	}

	#[test]
	fn test_select_one() {
		// Expect an authority in a set of 150 authorities.
		let a = select_one(seed_from_hashable(String::from("seed")), authority_set(150)).unwrap();
		// Expect a different value for different seed (collision is unlikely).
		let b = select_one(seed_from_hashable(String::from("seedy")), authority_set(150)).unwrap();
		assert_ne!(a, b);
		// If an empty set is provided, the result is `None`
		assert!(select_one::<u64>(seed_from_hashable(String::from("seed")), BTreeSet::default(),)
			.is_none());
	}

	fn assert_selected_subset_is_valid<T: Clone + Ord>(
		seed: u64,
		threshold: usize,
		set: BTreeSet<T>,
	) {
		let source = set.clone();
		let result = try_select_random_subset(seed, threshold, set).unwrap();
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
			assert_eq!(None, try_select_random_subset::<u64>(seed, 0, BTreeSet::default()));
			// threshold can't be larger than the set size
			assert_eq!(None, try_select_random_subset(seed, 6, (0..5).collect()));
		}
	}

	#[test]
	fn different_seed_different_set() {
		let set = (0..150).collect::<BTreeSet<_>>();
		for seed in 0..100 {
			// Note: strictly speaking these don't have to be different but the chances of a
			// collision should be quite low.
			assert_ne!(
				try_select_random_subset(seed, 100, set.clone()).unwrap(),
				try_select_random_subset(seed + 100, 100, set.clone()).unwrap(),
			);
		}
	}

	#[test]
	fn same_seed_same_set() {
		let set = (0..150).collect::<BTreeSet<_>>();
		for seed in 0..100 {
			assert_eq!(
				try_select_random_subset(seed, 100, set.clone()).unwrap(),
				try_select_random_subset(seed, 100, set.clone()).unwrap(),
			);
		}
	}
}
