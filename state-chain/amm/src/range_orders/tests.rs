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
		SqrtPriceQ64F96::from_tick(MIN_TICK),
		SqrtPriceQ64F96::from_tick(MAX_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
	BaseToQuote::output_amount_delta_floor(
		SqrtPriceQ64F96::from_tick(MAX_TICK),
		SqrtPriceQ64F96::from_tick(MIN_TICK),
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
						SqrtPriceQ64F96::from_raw(rng_u256_inclusive_bound(
							&mut rng,
							if tick > MIN_TICK {
								SqrtPriceQ64F96::from_tick(tick - 1).as_raw()..=
									SqrtPriceQ64F96::from_tick(tick).as_raw()
							} else {
								SqrtPriceQ64F96::from_tick(tick).as_raw()..=
									SqrtPriceQ64F96::from_tick(tick + 1).as_raw()
							},
						)),
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

mod fee_growth {

	use super::*;

	fn swap_until_tick(pool: &mut PoolState, tick: Tick) {
		let price = SqrtPriceQ64F96::from_tick(tick);

		let (output, _) = if pool.current_tick < tick {
			pool.swap::<QuoteToBase>(U256::MAX, Some(price))
		} else {
			pool.swap::<BaseToQuote>(U256::MAX, Some(price))
		};

		assert!(!output.is_zero(), "Swap should produce output");
		assert_eq!(pool.current_tick, tick);
	}

	/// Max liquidity to use when creating a new range order
	const MAX_LIQUIDITY: PoolPairsMap<U256> = PoolPairsMap {
		base: U256([10_000_000_000, 0, 0, 0]),
		quote: U256([10_000_000_000, 0, 0, 0]),
	};

	/// Fees are expected to be a fraction of provided liquidity
	const MAX_EXPECTED_FEES: PoolPairsMap<U256> =
		PoolPairsMap { base: U256([100_000_000, 0, 0, 0]), quote: U256([100_000_000, 0, 0, 0]) };

	fn create_new_position(
		pool: &mut PoolState,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
	) {
		let (_, _, collected, _) = pool
			.collect_and_mint(
				lp,
				lower_tick,
				upper_tick,
				Size::Amount { maximum: MAX_LIQUIDITY, minimum: Default::default() },
				Result::<_, Infallible>::Ok,
			)
			.unwrap();

		// Sanity check: new position should not yield any fees immediately after being created
		assert_eq!(collected.fees, Default::default());
	}

	fn collect_from_position_with_checks(
		pool: &mut PoolState,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
	) {
		let (_, _, collected, _) = pool
			.collect_and_mint(
				lp,
				lower_tick,
				upper_tick,
				Size::Liquidity { liquidity: 0 },
				Result::<_, Infallible>::Ok,
			)
			.unwrap();

		assert!(collected.fees < MAX_EXPECTED_FEES);
	}

	#[track_caller]
	fn close_position_with_checks(
		pool: &mut PoolState,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
	) {
		let (_, _, collected, _) = pool
			.collect_and_burn(
				lp,
				lower_tick,
				upper_tick,
				Size::Amount { maximum: MAX_LIQUIDITY, minimum: Default::default() },
			)
			.unwrap();

		assert!(collected.fees < MAX_EXPECTED_FEES);
	}

	/// Check if provided value has underflowed by checking if it is is *much* closer to U256::MAX
	/// than 0
	#[track_caller]
	fn ensure_underflowed(x: PoolPairsMap<U256>) {
		assert!(x > PoolPairsMap { base: (U256::MAX / 100) * 99, quote: (U256::MAX / 100) * 99 });
	}

	/// Check if provided value is *much* closer to 0 than U256::MAX
	#[track_caller]
	fn ensure_closer_to_zero(x: PoolPairsMap<U256>) {
		assert!(x < PoolPairsMap { base: U256::MAX / 1000000, quote: U256::MAX / 1000000 })
	}

	/// This test reproduces underflow in `collect_fees` (used to cause a panic) that occurs, for
	/// example, when:
	/// - A new position is created where the lower tick is NEW (doesn't exist)
	/// - The upper tick already EXISTS with `fee_growth_outside > 0`
	/// - The current price is between lower and upper
	#[test]
	fn fee_growth_inside_underflow() {
		let lp1 = LiquidityProvider::from([1; 32]);
		let lp2 = LiquidityProvider::from([2; 32]);
		let lp3 = LiquidityProvider::from([3; 32]);

		let mut pool = PoolState::new(1000, SqrtPriceQ64F96::from_tick(0)).unwrap();

		create_new_position(&mut pool, &lp1, 0, 100);
		create_new_position(&mut pool, &lp2, 0, 200);

		swap_until_tick(&mut pool, 150);
		swap_until_tick(&mut pool, 30);

		// Now tick 100's fee_growth_outside = positive value (fees while price was above 100)
		let tick_100_outside = pool.liquidity_map.get(&100).unwrap().fee_growth_outside;
		assert!(tick_100_outside > Default::default());

		// Create a NEW position [25, 100] for lp3
		// This is where the underflow would occur:
		// - Tick 25 is NEW (doesn't exist), current >= 25, so fee_growth_outside = global
		// - Tick 100 EXISTS with fee_growth_outside = X > 0
		// - Current tick is between 25 and 100
		//
		// In collect_fees:
		//   fee_growth_below = tick_25.outside = global
		//   fee_growth_above = tick_100.outside = X
		//   fee_growth_inside = global - global - X = -X  ← UNDERFLOW!
		// With the overflowing_sub fix, this succeeds.
		// Without the fix, this would panic.
		create_new_position(&mut pool, &lp3, 25, 100);

		// By convention, fee growth outside has been initialised to global fee growth
		assert_eq!(pool.liquidity_map.get(&25).unwrap().fee_growth_outside, pool.global_fee_growth);

		// Making sure that underflow did occur (the number is very close to U256::MAX):
		ensure_underflowed(
			pool.positions.get(&(lp3.clone(), 25, 100)).unwrap().last_fee_growth_inside,
		);

		// Collect all positions other than that of LP3 (and as a sanity check, make sure that fees
		// are smaller than the liquidity added):
		close_position_with_checks(&mut pool, &lp1, 0, 100);
		close_position_with_checks(&mut pool, &lp2, 0, 200);

		// Perform swaps to generate fees within [25, 100] range (no tick crossings)
		swap_until_tick(&mut pool, 75);
		swap_until_tick(&mut pool, 30);

		collect_from_position_with_checks(&mut pool, &lp3, 25, 100);

		ensure_underflowed(
			pool.positions.get(&(lp3.clone(), 25, 100)).unwrap().last_fee_growth_inside,
		);

		// Create another order using tick 25 (where the overflow happened):
		create_new_position(&mut pool, &lp1, 0, 25);

		// Its position's last_fee_growth_inside also underflowed:
		ensure_underflowed(
			pool.positions.get(&(lp1.clone(), 0, 25)).unwrap().last_fee_growth_inside,
		);

		// A few swaps to collect more fees
		swap_until_tick(&mut pool, 100);
		swap_until_tick(&mut pool, 10);

		collect_from_position_with_checks(&mut pool, &lp3, 25, 100);
		collect_from_position_with_checks(&mut pool, &lp1, 0, 25);

		// LP3's last growth has "recovered" from the underflow and is now a value closer to 0:
		ensure_closer_to_zero(
			pool.positions.get(&(lp3.clone(), 25, 100)).unwrap().last_fee_growth_inside,
		);

		ensure_underflowed(
			pool.positions.get(&(lp1.clone(), 0, 25)).unwrap().last_fee_growth_inside,
		);
	}

	/// Similar to the test above, except that the new tick is above the current tick,
	/// which means it will be initialised with 0, however it will still lead to an underflow.
	#[test]
	fn fee_growth_inside_underflow_new_tick_above_current() {
		let lp1 = LiquidityProvider::from([1; 32]);
		let lp2 = LiquidityProvider::from([2; 32]);
		let lp3 = LiquidityProvider::from([3; 32]);

		const NEW_TICK: Tick = 40;

		let mut pool = PoolState::new(1000, SqrtPriceQ64F96::from_tick(0)).unwrap();

		create_new_position(&mut pool, &lp1, 0, 100);
		create_new_position(&mut pool, &lp2, 0, 200);

		swap_until_tick(&mut pool, 150);
		swap_until_tick(&mut pool, 30);

		// Now tick 100's fee_growth_outside = positive value (fees while price was above 100)
		let tick_100_outside = pool.liquidity_map.get(&100).unwrap().fee_growth_outside;
		assert!(tick_100_outside > Default::default());

		create_new_position(&mut pool, &lp3, NEW_TICK, 100);

		// By convention, fee growth outside has been initialised to global fee growth
		assert_eq!(
			pool.liquidity_map.get(&NEW_TICK).unwrap().fee_growth_outside,
			Default::default()
		);

		// Making sure that underflow did occur (the number is very close to U256::MAX):
		ensure_underflowed(
			pool.positions
				.get(&(lp3.clone(), NEW_TICK, 100))
				.unwrap()
				.last_fee_growth_inside,
		);

		// Perform swaps to generate fees
		for _ in 0..2 {
			swap_until_tick(&mut pool, 100);
			swap_until_tick(&mut pool, 20);
		}

		// Make sure collected fees are reasonable:
		collect_from_position_with_checks(&mut pool, &lp3, NEW_TICK, 100);

		// Accrued fees should help us "recover" from the underflow
		ensure_closer_to_zero(
			pool.positions
				.get(&(lp3.clone(), NEW_TICK, 100))
				.unwrap()
				.last_fee_growth_inside,
		);
	}
}
