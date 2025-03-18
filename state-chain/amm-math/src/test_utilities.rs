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

#![cfg(feature = "slow-tests")]

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
