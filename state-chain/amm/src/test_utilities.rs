#![cfg(test)]

use rand::prelude::Distribution;
use sp_core::U256;

pub fn rng_u256_inclusive_bound(
	rng: &mut impl rand::Rng,
	bound: std::ops::RangeInclusive<U256>,
) -> U256 {
	let start = bound.start();
	let end = bound.end();

	let upper_start = (start >> 128).low_u128();
	let upper_end = (end >> 128).low_u128();

	if upper_start == upper_end {
		U256::from(
			rand::distributions::Uniform::new_inclusive(start.low_u128(), end.low_u128())
				.sample(rng),
		)
	} else {
		let upper = rand::distributions::Uniform::new_inclusive(upper_start, upper_end).sample(rng);
		let lower = if upper_start < upper && upper < upper_end {
			rng.gen()
		} else if upper_start == upper {
			rand::distributions::Uniform::new_inclusive(start.low_u128(), u128::MAX).sample(rng)
		} else {
			rand::distributions::Uniform::new_inclusive(0u128, end.low_u128()).sample(rng)
		};

		(U256::from(upper) << 128) + U256::from(lower)
	}
}
