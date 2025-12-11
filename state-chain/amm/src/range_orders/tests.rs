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

use rand::{prelude::Distribution, Rng, SeedableRng};

use crate::common::{BaseToQuote, Pairs, QuoteToBase};
use cf_amm_math::test_utilities::rng_u256_inclusive_bound;
#[cfg(feature = "slow-tests")]
use cf_amm_math::MIN_SQRT_PRICE;

use super::*;

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

#[test]
fn max_liquidity() {
	// Note a tick's liquidity_delta.abs() must be less than or equal to its gross liquidity,
	// and therefore <= MAX_TICK_GROSS_LIQUIDITY Also note that the total of all tick's deltas
	// must be zero. So the maximum possible liquidity is MAX_TICK_GROSS_LIQUIDITY * ((1 +
	// MAX_TICK - MIN_TICK) / 2) The divide by 2 comes from the fact that if for example all the
	// ticks from MIN_TICK to an including -1 had deltas of MAX_TICK_GROSS_LIQUIDITY, all the
	// other tick's deltas would need to be negative or zero to satisfy the requirement that the
	// sum of all deltas is zero. Importantly this means the current_liquidity can be
	// represented as a i128 as the maximum liquidity is less than half the maximum u128
	assert!(
		MAX_TICK_GROSS_LIQUIDITY
			.checked_mul((1 + MAX_TICK - MIN_TICK) as u128 / 2)
			.unwrap() < i128::MAX as u128
	);
}

#[test]
fn r_non_zero() {
	let smallest_initial_r = 0xfffcb933bd6fad37aa2d162d1a594001u128;
	assert!(
		(smallest_initial_r.ilog2() as i32 +
			(0xfff97272373d413259a46990580e213au128.ilog2() +
				0xfff2e50f5f656932ef12357cf3c7fdccu128.ilog2() +
				0xffe5caca7e10e4e61c3624eaa0941cd0u128.ilog2() +
				0xffcb9843d60f6159c9db58835c926644u128.ilog2() +
				0xff973b41fa98c081472e6896dfb254c0u128.ilog2() +
				0xff2ea16466c96a3843ec78b326b52861u128.ilog2() +
				0xfe5dee046a99a2a811c461f1969c3053u128.ilog2() +
				0xfcbe86c7900a88aedcffc83b479aa3a4u128.ilog2() +
				0xf987a7253ac413176f2b074cf7815e54u128.ilog2() +
				0xf3392b0822b70005940c7a398e4b70f3u128.ilog2() +
				0xe7159475a2c29b7443b29c7fa6e889d9u128.ilog2() +
				0xd097f3bdfd2022b8845ad8f792aa5825u128.ilog2() +
				0xa9f746462d870fdf8a65dc1f90e061e5u128.ilog2() +
				0x70d869a156d2a1b890bb3df62baf32f7u128.ilog2() +
				0x31be135f97d08fd981231505542fcfa6u128.ilog2() +
				0x9aa508b5b7a84e1c677de54f3e99bc9u128.ilog2() +
				0x5d6af8dedb81196699c329225ee604u128.ilog2() +
				0x2216e584f5fa1ea926041bedfe98u128.ilog2() +
				0x48a170391f7dc42444e8fa2u128.ilog2()) as i32 -
			(128 * 19)) >
			0
	);
}

#[test]
fn output_amounts_bounded() {
	// Note these values are significant over-estimates of the maximum output amount
	QuoteToBase::output_amount_delta_floor(
		sqrt_price_at_tick(MIN_TICK),
		sqrt_price_at_tick(MAX_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
	BaseToQuote::output_amount_delta_floor(
		sqrt_price_at_tick(MAX_TICK),
		sqrt_price_at_tick(MIN_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
}

#[cfg(feature = "slow-tests")]
#[test]
fn maximum_liquidity_swap() {
	use crate::range_orders::Size;

	let mut pool_state = PoolState::new(0, MIN_SQRT_PRICE).unwrap();

	let minted_amounts: PoolPairsMap<Amount> = (MIN_TICK..0)
		.map(|lower_tick| (lower_tick, -lower_tick))
		.map(|(lower_tick, upper_tick)| {
			pool_state
				.collect_and_mint(
					&LiquidityProvider::from([0; 32]),
					lower_tick,
					upper_tick,
					Size::Liquidity { liquidity: MAX_TICK_GROSS_LIQUIDITY },
					Result::<_, Infallible>::Ok,
				)
				.unwrap()
				.0
		})
		.fold(Default::default(), |acc, x| acc + x);

	let (output, _remaining) = pool_state.swap::<QuoteToBase>(Amount::MAX, None);

	assert!(((minted_amounts[Pairs::Base] - (MAX_TICK - MIN_TICK) /* Maximum rounding down by one per swap iteration */)..minted_amounts[Pairs::Base]).contains(&output));
}

#[test]
fn test_amounts_to_liquidity() {
	fn rng_tick_range(rng: &mut impl rand::Rng) -> (Tick, Tick) {
		let tick = rand::distributions::Uniform::new_inclusive(MIN_TICK, MAX_TICK).sample(rng);

		let upper_range = tick + 1..MAX_TICK + 1;
		let low_range = MIN_TICK..tick;

		if !upper_range.is_empty() {
			(tick, rng.gen_range(upper_range))
		} else {
			assert!(!low_range.is_empty());

			(rng.gen_range(low_range), tick)
		}
	}

	std::thread::scope(|scope| {
		for i in 0..1 {
			scope.spawn(move || {
				let mut rng: rand::rngs::StdRng = rand::rngs::StdRng::from_seed([i; 32]);

				// Iterations have been decreased to ensure tests run in a reasonable time, but
				// this has been run 100 billion times
				for _i in 0..1000000 {
					let tick = rng.gen_range(MIN_TICK..MAX_TICK);

					let pool_state = PoolState::new(
						0,
						rng_u256_inclusive_bound(
							&mut rng,
							if tick > MIN_TICK {
								sqrt_price_at_tick(tick - 1)..=sqrt_price_at_tick(tick)
							} else {
								sqrt_price_at_tick(tick)..=sqrt_price_at_tick(tick + 1)
							},
						),
					)
					.unwrap();

					let (lower, upper) = rng_tick_range(&mut rng);

					let original_liquidity =
						rand::distributions::Uniform::new_inclusive(0, MAX_TICK_GROSS_LIQUIDITY)
							.sample(&mut rng);

					let amounts = pool_state
						.inner_liquidity_to_amounts::<false>(original_liquidity, lower, upper)
						.0;

					let resultant_liquidity =
						pool_state.inner_amounts_to_liquidity(lower, upper, amounts);

					let maximum_error_from_rounding_amount =
						[amounts[Pairs::Base], amounts[Pairs::Quote]]
							.into_iter()
							.filter(|amount| !amount.is_zero())
							.map(|amount| {
								1f64 / (2f64.powf((256f64 - amount.leading_zeros() as f64) - 1f64))
							})
							.fold(0f64, f64::max);

					let maximum_error_from_rounding_liquidity = 1f64 / original_liquidity as f64;

					let error = u128::rem_euclid(
						u128::abs_diff(resultant_liquidity, original_liquidity),
						original_liquidity,
					) as f64 / (original_liquidity as f64);

					assert!(
						error <=
							maximum_error_from_rounding_amount +
								maximum_error_from_rounding_liquidity,
					);
				}
			});
		}
	});
}

/// This test reproduces the underflow issue (now fixed) in `collect_fees` that occurs when:
/// - A new position is created where the lower tick is NEW (doesn't exist)
/// - The upper tick already EXISTS with `fee_growth_outside > 0`
/// - The current price is between lower and upper
///
/// The bug: When a new tick is initialized with `fee_growth_outside = global_fee_growth`,
/// it claims "all fees occurred below this tick". But if an existing upper tick already
/// claims some fees occurred "above" it, the invariant `global = below + inside + above`
/// is violated, causing underflow when calculating `fee_growth_inside`.
#[test]
fn fee_growth_inside_underflow_new_tick_with_existing_tick() {
	let mut pool = PoolState::new(10000, sqrt_price_at_tick(50)).unwrap();

	let lp1 = LiquidityProvider::from([1; 32]);
	let lp2 = LiquidityProvider::from([2; 32]);
	let lp3 = LiquidityProvider::from([3; 32]);
	let lp4 = LiquidityProvider::from([4; 32]);

	let liquidity = 1_000_000_000_000_000u128;

	// Step 1: Create position [0, 100] for lp1
	// - Tick 0 (<= current 50): fee_growth_outside = global = 0
	// - Tick 100 (> current 50): fee_growth_outside = 0
	pool.collect_and_mint(&lp1, 0, 100, Size::Liquidity { liquidity }, Result::<_, Infallible>::Ok)
		.unwrap();

	// Step 2: Create position [50, 100] for lp2
	// This shares tick 100 with lp1, so tick 100 won't be removed when lp1 burns
	pool.collect_and_mint(
		&lp2,
		50,
		100,
		Size::Liquidity { liquidity },
		Result::<_, Infallible>::Ok,
	)
	.unwrap();

	// Step 3: Create position [100, 200] for lp4
	// This provides liquidity ABOVE tick 100 so fees can accumulate there
	// When price is above 100, swaps will generate fees in this range
	pool.collect_and_mint(
		&lp4,
		100,
		200,
		Size::Liquidity { liquidity },
		Result::<_, Infallible>::Ok,
	)
	.unwrap();

	// Step 4: Swap QuoteToBase to move price up past tick 100 to around 150
	// This crosses tick 100, and fees accumulate between 100-150
	let sqrt_price_at_150 = sqrt_price_at_tick(150);
	let (output1, _) = pool.swap::<QuoteToBase>(U256::MAX, Some(sqrt_price_at_150));
	assert!(!output1.is_zero(), "Swap should produce output");

	// Verify price moved above tick 100
	assert!(
		pool.current_tick >= 100,
		"Price should be at or above tick 100, got {}",
		pool.current_tick
	);

	// Step 5: Swap BaseToQuote to move price back down between 25 and 50
	// This crosses tick 100 again, updating its fee_growth_outside to reflect
	// the fees that accumulated while price was above 100
	let sqrt_price_at_30 = sqrt_price_at_tick(30);
	let (output2, _) = pool.swap::<BaseToQuote>(U256::MAX, Some(sqrt_price_at_30));
	assert!(!output2.is_zero(), "Swap should produce output");

	// Verify price is now around tick 30
	let current_tick = pool.current_tick;
	assert!(
		(25..50).contains(&current_tick),
		"Price should be between tick 25 and 50, got {}",
		current_tick
	);

	// Now tick 100's fee_growth_outside = positive value (fees while price was above 100)
	// This value is non-zero because lp4's position provided liquidity above tick 100
	let tick_100_outside = pool.liquidity_map.get(&100).unwrap().fee_growth_outside;
	assert!(
		!tick_100_outside[Pairs::Base].is_zero() || !tick_100_outside[Pairs::Quote].is_zero(),
		"Tick 100 should have non-zero fee_growth_outside after fees accumulated above it"
	);

	// Step 6: Burn lp1's position [0, 100]
	// - Tick 0 is removed (only lp1 was using it)
	// - Tick 100 stays (lp2 and lp4 are still using it)
	pool.collect_and_burn(&lp1, 0, 100, Size::Liquidity { liquidity }).unwrap();

	// Verify tick 0 is gone but tick 100 still exists
	assert!(!pool.liquidity_map.contains_key(&0), "Tick 0 should be removed");
	assert!(pool.liquidity_map.contains_key(&100), "Tick 100 should still exist");

	// Step 7: Create a NEW position [25, 100] for lp3
	// This is where the underflow would occur:
	// - Tick 25 is NEW (doesn't exist), current >= 25, so fee_growth_outside = global
	// - Tick 100 EXISTS with fee_growth_outside = X > 0
	// - Current tick is between 25 and 100
	//
	// In collect_fees:
	//   fee_growth_below = tick_25.outside = global
	//   fee_growth_above = tick_100.outside = X
	//   fee_growth_inside = global - global - X = -X  ← UNDERFLOW!
	//
	// With the saturating_sub fix, this succeeds.
	// Without the fix, this would panic.
	let result = pool.collect_and_mint(
		&lp3,
		25,
		100,
		Size::Liquidity { liquidity: 1_000_000 },
		Result::<_, Infallible>::Ok,
	);

	assert!(result.is_ok(), "Position creation should succeed with wrapping fix");
	pool.collect_and_burn(&lp2, 50, 100, Size::Liquidity { liquidity }).unwrap();
	pool.collect_and_burn(&lp4, 100, 200, Size::Liquidity { liquidity }).unwrap();

	// Now only lp3's position [25, 100] is active
	let lp3_liquidity: U256 = 1_000_000u128.into();

	// Capture global_fee_growth BEFORE swaps
	let global_fee_growth_before = pool.global_fee_growth;

	// Perform swaps that stay within [25, 100] range (no tick crossings)
	let sqrt_price_at_75 = sqrt_price_at_tick(75);
	let (output2, _) = pool.swap::<QuoteToBase>(U256::MAX, Some(sqrt_price_at_75));
	assert!(!output2.is_zero(), "Swap should produce output");

	let (output3, _) = pool.swap::<BaseToQuote>(U256::MAX, Some(sqrt_price_at_30));
	assert!(!output3.is_zero(), "Swap should produce output");

	// Capture global_fee_growth AFTER swaps
	let global_fee_growth_after = pool.global_fee_growth;

	// Collect fees by minting with 0 additional liquidity
	let (_, _, collected, _) = pool
		.collect_and_mint(
			&lp3,
			25,
			100,
			Size::Liquidity { liquidity: 0 },
			Result::<_, Infallible>::Ok,
		)
		.unwrap();

	// Verify: collected_fees = Δglobal_fee_growth × liquidity / 2¹²⁸
	// Since fee_growth is in Q128.128 format, we use mul_div_floor
	for side in [Pairs::Base, Pairs::Quote] {
		let delta_global =
			global_fee_growth_after[side].overflowing_sub(global_fee_growth_before[side]).0;
		println!("{:?}", delta_global);
		// expected_fees = delta_global × liquidity / 2^128
		let expected_fees = mul_div_floor(delta_global, lp3_liquidity, U512::one() << 128);
		println!("{:?}", expected_fees);
		assert_eq!(
			collected.fees[side], expected_fees,
			"Collected fees for {:?} should equal Δglobal × liquidity / 2¹²⁸.\n\
			 Δglobal_fee_growth: {:?}\n\
			 liquidity: {:?}\n\
			 expected: {:?}\n\
			 actual: {:?}",
			side, delta_global, lp3_liquidity, expected_fees, collected.fees[side]
		);
	}

	println!(
		"✓ Fee collection verified: collected fees match Δglobal_fee_growth × liquidity / 2¹²⁸"
	);
}
