use super::*;
use frame_support::sp_runtime::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};

/// Distributes exactly `total_to_distribute` proportionally to the `distribution` map.
pub(super) fn distribute_proportionally<'a, K, N, I>(
	total_to_distribute: N,
	distribution: I,
) -> BTreeMap<&'a K, N>
where
	N: Clone
		+ From<u64>
		+ Copy
		+ core::ops::AddAssign
		+ frame_support::sp_runtime::Saturating
		+ frame_support::sp_runtime::traits::AtLeast32BitUnsigned,
	K: Ord,
	u128: From<N> + UniqueSaturatedInto<N>,
	I: Iterator<Item = (&'a K, u128)> + Clone,
{
	use nanorand::Rng;

	let total = distribution
		.clone()
		.try_fold(0u128, |acc, (_, v)| acc.checked_add(v))
		// Overflow should be unexpected, but this ensures we don't create money out of thin
		// air (division by zero is handled gracefully below too):
		.unwrap_or_default();

	let mut total_distributed: N = 0u32.into();

	let mut distribution: BTreeMap<_, _> = distribution
		.map(|(k, v)| {
			let amount: N = multiply_by_rational_with_rounding(
				total_to_distribute.into(),
				v,
				total,
				Rounding::Down,
			)
			.unwrap_or_default()
			.unique_saturated_into();

			total_distributed += amount;
			(k, amount)
		})
		.collect();

	// Due to always rounding down we may have a small amount left over, give it a random key
	let remaining_to_distribute = total_to_distribute.saturating_sub(total_distributed);
	let lucky_index = {
		// Convert to u64 by ignoring high bits
		let seed = u128::from(total_to_distribute) as u64;
		nanorand::WyRand::new_seed(seed).generate_range(0..distribution.len())
	};
	if let Some((_lp_id, amount)) = distribution.iter_mut().nth(lucky_index) {
		amount.saturating_accrue(remaining_to_distribute);
	}

	distribution
}

#[test]
fn distribute_proportionally_test() {
	// A single party should get everything:
	assert_eq!(
		distribute_proportionally(100u128, BTreeMap::from([(1, 999)]).iter().map(|(k, v)| (k, *v))),
		BTreeMap::from([(&1, 100)])
	);

	// Distributes proportionally:
	assert_eq!(
		distribute_proportionally(
			1000u128,
			BTreeMap::from([(1, 33), (2, 50), (3, 17)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 330), (&2, 500), (&3, 170)])
	);

	// Handles rounding errors in a reasonable way:
	assert_eq!(
		distribute_proportionally(
			1000u128,
			BTreeMap::from([(1, 100), (2, 100), (3, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 333), (&2, 333), (&3, 334)])
	);

	// Some extreme cases:
	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			1000u128,
			BTreeMap::<u32, u128>::from([]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			0u128,
			BTreeMap::from([(1, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 0)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			1000u128,
			BTreeMap::from([(1, 0), (2, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 0), (&2, 1000)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			u128::MAX,
			BTreeMap::from([(1, 100), (2, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, u128::MAX / 2), (&2, u128::MAX / 2 + 1)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			1000u128,
			BTreeMap::from([(1, u128::MAX), (2, u128::MAX)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 0), (&2, 1000)])
	);
}
