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

use crate::{limit_orders, range_orders};
use cf_amm_math::{
	mul_div, mul_div_ceil_checked, mul_div_floor_checked, test_utilities::rng_u256_inclusive_bound,
	MAX_TICK, MIN_TICK,
};

use super::*;

use cf_utilities::{assert_matches, assert_ok, assert_panics};
use rand::{Rng, SeedableRng};

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

fn mul_div_ceil(a: U256, b: U256, c: U256) -> U256 {
	mul_div_ceil_checked(a, b, c).unwrap()
}

fn mul_div_floor(a: U256, b: U256, c: U256) -> U256 {
	mul_div_floor_checked(a, b, c).unwrap()
}

/// Only the `remove_fixed_pools` migration still reads a [FloatBetweenZeroAndOne]. Delete this
/// along with that migration and the type itself.
#[test]
fn test_float() {
	let mut rng = rand::rngs::StdRng::from_seed([8u8; 32]);

	fn rng_u256(rng: &mut impl rand::Rng) -> U256 {
		U256([(); 4].map(|()| rng.gen()))
	}

	fn rng_u256_numerator_denominator(rng: &mut impl rand::Rng) -> (U256, U256) {
		let numerator = rng_u256(rng);
		(numerator, rng_u256_inclusive_bound(rng, numerator..=U256::MAX))
	}

	for x in std::iter::repeat_n((), 16).map(|_| rng_u256(&mut rng)) {
		assert_eq!(FloatBetweenZeroAndOne::max(), FloatBetweenZeroAndOne::max().mul_div_ceil(x, x));
	}

	for ((x, y), z) in std::iter::repeat_n((), 16)
		.map(|_| (rng_u256_numerator_denominator(&mut rng), rng_u256(&mut rng)))
	{
		let f = FloatBetweenZeroAndOne::max().mul_div_ceil(x, y);

		assert_eq!(Some((z, z)), FloatBetweenZeroAndOne::integer_mul_div(z, &f, &f));
	}

	for ((x, y), z) in
		(0..16).map(|_| (rng_u256_numerator_denominator(&mut rng), rng_u256(&mut rng)))
	{
		let (floor, ceil) = FloatBetweenZeroAndOne::integer_mul_div(
			z,
			&FloatBetweenZeroAndOne::max().mul_div_ceil(x, y),
			&FloatBetweenZeroAndOne::max(),
		)
		.unwrap();
		let (bound_floor, bound_ceil) = mul_div(z, x, y);

		assert!(floor >= bound_floor && ceil >= bound_ceil);
	}

	for _ in 0..1024 {
		let initial_value = rng_u256(&mut rng);
		let initial_float = FloatBetweenZeroAndOne::max();

		let (final_value_floor, final_value_ceil, final_float) = (0..rng.gen_range(8..256))
			.map(|_| rng_u256_numerator_denominator(&mut rng))
			.fold(
				(initial_value, initial_value, initial_float.clone()),
				|(value_floor, value_ceil, float), (n, d)| {
					(
						mul_div_floor(value_floor, n, d),
						mul_div_ceil(value_ceil, n, d),
						float.mul_div_ceil(n, d),
					)
				},
			);

		let final_value_via_float =
			FloatBetweenZeroAndOne::integer_mul_div(initial_value, &final_float, &initial_float)
				.unwrap()
				.0;

		assert!(final_value_ceil >= final_value_via_float);
		assert!(final_value_floor <= final_value_via_float);
	}

	{
		let low_mantissa =
			FloatBetweenZeroAndOne::max().mul_div_ceil(U256::one() << 255, U256::MAX);
		let high_mantissa = low_mantissa.mul_div_ceil(U256::MAX >> 1, U256::one() << 255);

		assert!(low_mantissa.normalised_mantissa < high_mantissa.normalised_mantissa);
		assert_eq!(
			FloatBetweenZeroAndOne::integer_mul_div(U256::MAX, &high_mantissa, &low_mantissa),
			Some((U256::MAX - 2, U256::MAX - 1))
		);
	}

	{
		let min_mantissa =
			FloatBetweenZeroAndOne::max().mul_div_ceil(U256::one() << 255, U256::MAX);

		assert_eq!(min_mantissa.normalised_mantissa, U256::one() << 255);

		let float = min_mantissa.mul_div_ceil(U256::one(), U256::MAX);

		assert_eq!(float.negative_exponent, U256::from(256));
		assert_eq!(float.normalised_mantissa, (U256::one() << 255) + U256::one());
	}

	{
		assert_panics!(FloatBetweenZeroAndOne::max().mul_div_ceil(2.into(), 1.into()));
		assert_panics!(
			FloatBetweenZeroAndOne::max().mul_div_ceil(U256::MAX, U256::MAX / U256::from(2))
		);
		assert_panics!(FloatBetweenZeroAndOne::max().mul_div_ceil(0.into(), 1.into()));
	}

	{
		// A numerator above the denominator means a price with more left than when the order
		// recorded its share of it, which the old representation could not produce. Refused
		// rather than panicking, so one corrupt entry cannot halt the chain mid-migration.
		assert_eq!(
			FloatBetweenZeroAndOne::integer_mul_div(
				1.into(),
				&FloatBetweenZeroAndOne::max(),
				&FloatBetweenZeroAndOne::max().mul_div_ceil(1.into(), 2.into())
			),
			None
		);
		assert_eq!(
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::MAX,
				&FloatBetweenZeroAndOne::max(),
				&FloatBetweenZeroAndOne::max().mul_div_ceil(1.into(), 2.into())
			),
			None
		);
		// Nor could it produce a mantissa without its top bit set, i.e. one not normalised.
		assert_eq!(
			FloatBetweenZeroAndOne::integer_mul_div(
				1000.into(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::one(),
					negative_exponent: U256::zero()
				},
				&FloatBetweenZeroAndOne::max()
			),
			None
		);
	}

	fn min_float() -> FloatBetweenZeroAndOne {
		FloatBetweenZeroAndOne {
			negative_exponent: U256::MAX,
			normalised_mantissa: U256::one() << 255,
		}
	}

	{
		assert_eq!(min_float(), min_float().mul_div_ceil(U256::one(), U256::from(256)));
		assert_eq!(min_float(), min_float().mul_div_ceil(U256::MAX - 1, U256::MAX));
	}

	{
		assert_eq!(
			Some((U256::zero(), U256::one())),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::MAX,
				&min_float(),
				&FloatBetweenZeroAndOne::max()
			)
		);
		assert_eq!(
			Some((U256::zero(), U256::one())),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::one(),
				&min_float(),
				&FloatBetweenZeroAndOne::max()
			)
		);
		assert_eq!(
			Some((U256::MAX, U256::MAX)),
			FloatBetweenZeroAndOne::integer_mul_div(U256::MAX, &min_float(), &min_float())
		);
		assert_eq!(
			Some((U256::one() << 255, U256::one() << 255)),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::MAX,
				&min_float(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::MAX,
					negative_exponent: U256::MAX
				}
			)
		);
		assert_eq!(
			Some((U256::zero(), U256::zero())),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::zero(),
				&min_float(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::MAX,
					negative_exponent: U256::MAX
				}
			)
		);
		assert_eq!(
			Some((U256::zero(), U256::one())),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::one(),
				&min_float(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::MAX,
					negative_exponent: U256::MAX
				}
			)
		);
	}

	{
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, U256::MAX),
			(U512::zero(), U512::MAX)
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 512.into()),
			(U512::zero(), U512::MAX)
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 511.into()),
			(U512::one(), U512::MAX >> 1)
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 256.into()),
			(U256::MAX.into(), U256::MAX.into())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 255.into()),
			(U512::MAX >> 255, (U256::MAX >> 1).into())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), U256::MAX),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 512.into()),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 511.into()),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 255.into()),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 128.into()),
			(U512::zero(), U512::zero())
		);
	}
}

/// A distinct liquidity provider per id. Ids sort the same way their providers do, which is the
/// order the book stores their orders in.
fn lp(id: u16) -> LiquidityProvider {
	let mut key = [0u8; 32];
	key[0..2].copy_from_slice(&id.to_be_bytes());
	LiquidityProvider::from(key)
}

fn total_of(
	fills: &[Fill<LiquidityProvider>],
	f: impl Fn(&Fill<LiquidityProvider>) -> Amount,
) -> Amount {
	fills.iter().fold(Amount::zero(), |total, fill| total + f(fill))
}

/// A uniformly random `Amount` in `1..=max`. `rand` only ranges over primitives, and the sizes
/// worth testing here run past `u128`, so draw the whole `U256` and fold it into the range.
fn random_amount(rng: &mut impl rand::Rng, max: Amount) -> Amount {
	let mut bytes = [0u8; 32];
	rng.fill(&mut bytes[..]);
	Amount::from_little_endian(&bytes) % max + Amount::one()
}

/// A selection that leaves every order taking part, i.e. the behaviour of a swap against a book
/// no larger than one swap will settle.
fn all_participating(orders: &Orders<LiquidityProvider>) -> Participants<LiquidityProvider> {
	Participants::select(orders, &FillQuota::Unlimited, Amount::MAX)
		.expect("the test books all hold orders")
}

/// Performs a swap that will fill every order it reaches, checking the invariants.
fn swap<SD: SwapDirection>(
	pool_state: &mut PoolState,
	amount: Amount,
	sqrt_price_limit: Option<SqrtPrice>,
) -> (Amount, Amount, Vec<Fill<LiquidityProvider>>) {
	swap_with_fill_quota::<SD>(pool_state, amount, sqrt_price_limit, FillQuota::Unlimited)
}

/// Performs a swap with the given fill quota, checking the invariants.
fn swap_with_fill_quota<SD: SwapDirection>(
	pool_state: &mut PoolState,
	amount: Amount,
	sqrt_price_limit: Option<SqrtPrice>,
	quota: FillQuota,
) -> (Amount, Amount, Vec<Fill<LiquidityProvider>>) {
	let mut quota = quota;
	let super::SwapOutcome {
		report:
			crate::SwapReport {
				outcome:
					crate::SwapExecutionOutcome {
						output_amount,
						limit_order_fills: fills,
						limit_order_input_dust,
						..
					},
				remaining_input_amount: remaining_amount,
				..
			},
		..
	} = pool_state.swap::<SD>(amount, sqrt_price_limit, 0, &mut quota, None);

	assert_eq!(
		total_of(&fills, |fill| fill.sold_amount),
		output_amount,
		"the orders must give up exactly what the swap bought"
	);
	assert_eq!(
		total_of(&fills, |fill| fill.bought_amount) + limit_order_input_dust,
		amount - remaining_amount,
		"LP proceeds and protocol dust must account for all consumed input"
	);

	(output_amount, remaining_amount, fills)
}

/// Orders far larger than the swap split it between them and keep the rest. The share arithmetic
/// is bounded by its `U512` intermediates, not by the size of the orders.
#[test]
fn huge_orders_are_partially_filled_pro_rata() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection>() {
		let quarter = Amount::MAX / 4;
		let mut pool_state = PoolState::new();
		assert_ok!(pool_state.mint::<SD>(&lp(0), 0, quarter));
		assert_ok!(pool_state.mint::<SD>(&lp(1), 0, quarter));

		// Tick zero prices one for one, and the orders are equal, so the swap buys what it pays
		// in and the two split it evenly.
		let (output, remaining, fills) = swap::<SD>(&mut pool_state, 1_000_000.into(), None);

		assert_eq!(output, 1_000_000.into());
		assert!(remaining.is_zero());
		assert_eq!(fills.len(), 2);
		assert!(fills.iter().all(|fill| fill.sold_amount == 500_000.into()));
		assert_eq!(pool_state.liquidity::<SD>(), vec![(0, quarter * 2 - output)]);
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();
}

/// At the extreme prices, converting a price's whole liquidity overflows in one direction and
/// floors to nothing in the other. Neither may panic, and neither may hand out more than the price
/// holds.
#[test]
fn extreme_prices_do_not_panic_however_large_the_order() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection>(tick: Tick) {
		let mut pool_state = PoolState::new();
		let minted = Amount::MAX / 2;
		assert_ok!(pool_state.mint::<SD>(&lp(0), tick, minted));

		let (output, _remaining, _fills) = swap::<SD>(&mut pool_state, Amount::MAX / 2, None);

		assert!(output <= minted, "a swap cannot buy more than the price holds");
	}

	for tick in [MIN_TICK, MAX_TICK] {
		inner::<BaseToQuote>(tick);
		inner::<QuoteToBase>(tick);
	}
}

#[test]
fn mint() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		for good in [MIN_TICK, MAX_TICK] {
			let mut pool_state = PoolState::new();
			assert_eq!(
				assert_ok!(pool_state.mint::<SD>(&lp(0), good, 1000.into())),
				Position::new(1000.into())
			);
		}

		for bad in [MIN_TICK - 1, MAX_TICK + 1] {
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.mint::<SD>(&lp(0), bad, 1000.into()),
				Err(PositionError::InvalidTick)
			);
		}

		// No amount is too large to mint: an invalid tick is the only way minting fails.
		for good in [Amount::one(), Amount::MAX / 2, Amount::MAX] {
			let mut pool_state = PoolState::new();
			assert_eq!(assert_ok!(pool_state.mint::<SD>(&lp(0), 0, good)), Position::new(good));
		}

		// Minting nothing reports the existing order, and errors if there isn't one.
		{
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.mint::<SD>(&lp(0), 0, Amount::zero()),
				Err(PositionError::NonExistent)
			);
			assert_ok!(pool_state.mint::<SD>(&lp(0), 0, 1000.into()));
			assert_eq!(
				assert_ok!(pool_state.mint::<SD>(&lp(0), 0, Amount::zero())),
				Position::new(1000.into())
			);
		}
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();
}

#[test]
fn burn() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		{
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.burn::<SD>(&lp(0), MIN_TICK - 1, 1000.into()),
				Err(PositionError::InvalidTick)
			);
			assert_matches!(
				pool_state.burn::<SD>(&lp(0), MAX_TICK + 1, 1000.into()),
				Err(PositionError::InvalidTick)
			);
		}
		{
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.burn::<SD>(&lp(0), 120, 1000.into()),
				Err(PositionError::NonExistent)
			);
		}
		{
			let mut pool_state = PoolState::new();
			let tick = 120;
			let amount = U256::from(1000);
			assert_eq!(
				assert_ok!(pool_state.mint::<SD>(&lp(0), tick, amount)),
				Position::new(amount)
			);
			assert_eq!(
				assert_ok!(pool_state.burn::<SD>(&lp(0), tick, amount)),
				(amount, Position::default())
			);
			// Burning an order in its entirety removes it.
			assert_eq!(pool_state.position::<SD>(&lp(0), tick), None);
		}
		{
			// Burning one lp's order leaves the others at that price alone.
			let mut pool_state = PoolState::new();
			let tick = 120;
			let amount = U256::from(1000);
			assert_ok!(pool_state.mint::<SD>(&lp(1), tick, 56.into()));
			assert_eq!(
				assert_ok!(pool_state.mint::<SD>(&lp(0), tick, amount)),
				Position::new(amount)
			);
			assert_ok!(pool_state.mint::<SD>(&lp(2), tick, 16.into()));
			assert_eq!(
				assert_ok!(pool_state.burn::<SD>(&lp(0), tick, amount)),
				(amount, Position::default())
			);
			assert_eq!(pool_state.liquidity::<SD>(), vec![(tick, (56 + 16).into())]);
		}
		{
			// An order bought in its entirety no longer exists, so there is nothing left to burn.
			let mut pool_state = PoolState::new();
			let tick = 0;
			let amount = U256::from(1000);
			assert_ok!(pool_state.mint::<SD>(&lp(0), tick, amount));

			let (output, remaining, fills) = swap::<SD>(&mut pool_state, amount, None);
			assert_eq!(output, amount);
			assert_eq!(remaining, Amount::zero());
			assert_eq!(
				fills,
				vec![Fill {
					lp: lp(0),
					tick,
					sold_amount: amount,
					bought_amount: amount,
					remaining_amount: Amount::zero(),
				}]
			);

			assert_matches!(
				pool_state.burn::<SD>(&lp(0), tick, Amount::zero()),
				Err(PositionError::NonExistent)
			);
		}
		{
			// A partially bought order can still be burnt, down to what is left of it.
			let mut pool_state = PoolState::new();
			let tick = 0;
			let amount = U256::from(1000);
			let swapped = U256::from(600);
			assert_ok!(pool_state.mint::<SD>(&lp(0), tick, amount));

			let (output, _remaining, fills) = swap::<SD>(&mut pool_state, swapped, None);
			assert_eq!(output, swapped);
			assert_eq!(
				fills,
				vec![Fill {
					lp: lp(0),
					tick,
					sold_amount: swapped,
					bought_amount: swapped,
					remaining_amount: amount - swapped,
				}]
			);

			assert_eq!(
				assert_ok!(pool_state.burn::<SD>(&lp(0), tick, amount)),
				(amount - swapped, Position::default())
			);
		}
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();
}

#[test]
fn swap_consumes_orders() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		let swapped = U256::from(20);
		// Tick zero is a price of one, and limit orders charge no fee, so a partial fill is
		// exactly the amount swapped.
		let output = swapped;
		{
			let mut pool_state = PoolState::new();
			assert_ok!(pool_state.mint::<SD>(&lp(0), 0, 1000.into()));
			assert_eq!(swap::<SD>(&mut pool_state, swapped, None).0, output);
		}
		{
			// One lp with the same order minted twice.
			let mut pool_state = PoolState::new();
			let tick = 0;
			assert_ok!(pool_state.mint::<SD>(&lp(0), tick, 500.into()));
			assert_ok!(pool_state.mint::<SD>(&lp(0), tick, 500.into()));
			assert_eq!(swap::<SD>(&mut pool_state, swapped, None).0, output);
		}
		{
			// Two lps at the same price.
			let mut pool_state = PoolState::new();
			let tick = 0;
			assert_ok!(pool_state.mint::<SD>(&lp(1), tick, 500.into()));
			assert_ok!(pool_state.mint::<SD>(&lp(2), tick, 500.into()));
			assert_eq!(swap::<SD>(&mut pool_state, swapped, None).0, output);
		}
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();

	// Partial liquidity, multiple prices
	{
		let tick = 0;
		for (range, offset) in [
			(U256::from(149998000)..=U256::from(150000000), 0),
			(U256::from(150000000)..=U256::from(150002000), 1),
		] {
			let mut pool_state = PoolState::new();
			assert_ok!(pool_state.mint::<BaseToQuote>(&lp(0), tick, 100000000.into()));
			assert_ok!(pool_state.mint::<BaseToQuote>(
				&lp(0),
				offset +
					SqrtPrice::try_from_raw(
						SqrtPrice::from_tick(tick).as_raw() * U256::from(4).integer_sqrt()
					)
					.unwrap()
					.to_tick(),
				100000000.into()
			));
			let (output, remaining, _fills) =
				swap::<BaseToQuote>(&mut pool_state, 75000000.into(), None);
			assert!(range.contains(&output));
			assert_eq!(remaining, Amount::zero());
		}
	}
	{
		let tick = 0;
		for (range, offset) in [
			(U256::from(120000000)..=U256::from(120002000), 0),
			(U256::from(119998000)..=U256::from(120000000), 1),
		] {
			let mut pool_state = PoolState::new();
			assert_ok!(pool_state.mint::<QuoteToBase>(&lp(0), tick, 100000000.into()));
			assert_ok!(pool_state.mint::<QuoteToBase>(
				&lp(0),
				offset +
					SqrtPrice::try_from_raw(
						SqrtPrice::from_tick(tick).as_raw() * U256::from(4).integer_sqrt()
					)
					.unwrap()
					.to_tick(),
				100000000.into()
			));
			let (output, remaining, _fills) =
				swap::<QuoteToBase>(&mut pool_state, 180000000.into(), None);
			assert!(range.contains(&output));
			assert_eq!(remaining, Amount::zero());
		}
	}

	// All liquidity, multiple prices
	{
		let mut pool_state = PoolState::new();
		let tick = 0;
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(0), tick, 100.into()));
		assert_ok!(pool_state.mint::<BaseToQuote>(
			&lp(0),
			SqrtPrice::try_from_raw(
				SqrtPrice::from_tick(tick).as_raw() * U256::from(4).integer_sqrt()
			)
			.unwrap()
			.to_tick(),
			100.into()
		));
		let (output, remaining, _fills) = swap::<BaseToQuote>(&mut pool_state, 150.into(), None);
		assert_eq!((output, remaining), (200.into(), 24.into()));
	}
	{
		let mut pool_state = PoolState::new();
		let tick = 0;
		assert_ok!(pool_state.mint::<QuoteToBase>(&lp(0), tick, 100.into()));
		assert_ok!(pool_state.mint::<QuoteToBase>(
			&lp(0),
			SqrtPrice::try_from_raw(
				SqrtPrice::from_tick(tick).as_raw() * U256::from(4).integer_sqrt()
			)
			.unwrap()
			.to_tick(),
			100.into()
		));
		let (output, remaining, _fills) = swap::<QuoteToBase>(&mut pool_state, 550.into(), None);
		assert_eq!((output, remaining), (200.into(), 50.into()));
	}
}

/// Orders at the same price are filled in proportion to the liquidity each of them provides.
#[test]
fn fills_are_split_pro_rata() {
	// 1.0001^6932 ~ 2, so this input buys exactly twice as much as it pays in.
	let tick = 6932;
	let mut pool_state = PoolState::new();
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(0), tick, 1000.into()));
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(1), tick, 2000.into()));
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(2), tick, 7000.into()));

	let (output, remaining, fills) = swap::<BaseToQuote>(&mut pool_state, 1000.into(), None);
	assert_eq!((output, remaining), (2000.into(), Amount::zero()));

	// Largest first.
	assert_eq!(
		fills,
		vec![
			Fill {
				lp: lp(2),
				tick,
				sold_amount: 1400.into(),
				bought_amount: 700.into(),
				remaining_amount: 5600.into(),
			},
			Fill {
				lp: lp(1),
				tick,
				sold_amount: 400.into(),
				bought_amount: 200.into(),
				remaining_amount: 1600.into(),
			},
			Fill {
				lp: lp(0),
				tick,
				sold_amount: 200.into(),
				bought_amount: 100.into(),
				remaining_amount: 800.into(),
			},
		]
	);
}

// Tick 69081 is approximately 100,000 USDC/BTC with eight/six decimals. Five hundred
// small orders ahead of a large one must not amplify its underpayment across swaps.
fn check_fragmented_book_fills<SD: SwapDirection>(small_order_amount: u128, input: u128) {
	let tick = 69_081;
	let price = Price::from_tick(tick).unwrap();
	let small_order_count = 500u16;
	let order_lp = |id: u16| {
		let mut account = [0u8; 32];
		account[..2].copy_from_slice(&id.to_be_bytes());
		LiquidityProvider::from(account)
	};
	let last_lp = order_lp(small_order_count);

	let mut pool_state = PoolState::new();
	let mut totals = BTreeMap::new();

	// Five hundred small orders precede one order with 500 times their individual liquidity.
	for id in 0..=small_order_count {
		let lp = order_lp(id);
		let amount = Amount::from(small_order_amount) *
			Amount::from(if lp == last_lp { small_order_count } else { 1 });
		assert_ok!(pool_state.mint::<SD>(&lp, tick, amount));
		totals.insert(lp, (amount, Amount::zero(), Amount::zero(), 0u64));
	}

	// Across fifty swaps, each LP's shortfall must stay below one proceeds unit per fill,
	// regardless of how many orders precede it.
	for _ in 0..50 {
		let (output, remaining, fills) = swap::<SD>(&mut pool_state, input.into(), None);
		// Each swap has to execute in full.
		assert!(!output.is_zero());
		assert!(remaining.is_zero());
		for fill in fills {
			assert_eq!(fill.tick, tick);
			assert!(
				fill.bought_amount >= SD::input_amount_floor(fill.sold_amount, price).unwrap(),
				"every order must receive at least its rounded-down limit value"
			);

			// Compare the cross-multiplication of the fill's sold and bought amounts to avoid
			// rounding errors.
			let paid = fill.bought_amount.full_mul(output);
			let owed = fill.sold_amount.full_mul(input.into());
			// Every order, including the last, is short by less than one proceeds unit:
			// (owed - paid) / output < 1.
			assert!(paid <= owed, "no LP may collect another order's dust");
			assert!(owed - paid < U512::from(output));

			// The fill's account of the order has to match the order the pool is left holding.
			let (original, sold, bought, count) = totals.get_mut(&fill.lp).unwrap();
			*sold += fill.sold_amount;
			*bought += fill.bought_amount;
			*count += 1;
			assert_eq!(fill.remaining_amount, *original - *sold);
			match pool_state.position::<SD>(&fill.lp, tick) {
				Some(position) => {
					assert_eq!(position.amount, fill.remaining_amount);
					assert_eq!(position.original_amount, *original);
				},
				None => assert!(fill.remaining_amount.is_zero()),
			}
		}

		// Each order can lose less than one proceeds unit per fill; its cumulative allowance grows
		// with the number of fills, even though no individual fill violates its rounded limit.
		for (_, sold, bought, count) in totals.values() {
			if *count == 0 {
				continue
			}
			assert!(*bought + Amount::from(*count) > SD::input_amount_floor(*sold, price).unwrap());
		}
	}
}

#[test]
fn repeated_fragmented_btc_sales_respect_the_limit_price() {
	// $10 of BTC per small order; each swap spends 100.50 USDC.
	check_fragmented_book_fills::<QuoteToBase>(10_000, 100_500_000);
}

#[test]
fn repeated_fragmented_usdc_sales_respect_the_limit_price() {
	// $10 of USDC per small order; each swap spends 100,501 satoshis.
	check_fragmented_book_fills::<BaseToQuote>(10_000_000, 100_501);
}

#[test]
fn zero_output_swap_withholds_input_without_filling_orders() {
	fn inner<SD: SwapDirection>(tick: Tick, order_amount: u128) {
		let mut pool_state = PoolState::new();
		for id in 0..3 {
			assert_ok!(pool_state.mint::<SD>(&lp(id), tick, order_amount.into()));
		}
		let before = pool_state.orders.clone();

		let (output, remaining, fills) = swap::<SD>(&mut pool_state, Amount::one(), None);
		assert!(output.is_zero());
		assert!(remaining.is_zero(), "the swap must consume its input and terminate");
		assert_eq!(pool_state.orders, before);
		assert!(fills.is_empty());
	}

	// One micro-USDC cannot buy a satoshi; one wei cannot buy a micro-USDC.
	inner::<QuoteToBase>(69_081, 10_000);
	inner::<BaseToQuote>(-196_256, 10_000_000);
}

/// The `swap` helper asserts that fills account for a swap exactly, but the books above are all
/// hand-picked round numbers. Distribution is arithmetic over arbitrary sizes, so drive arbitrary
/// ones through it: a unit conjured up is an lp paid for liquidity that never existed, and a unit
/// lost is an lp's liquidity vanishing off the book.
#[test]
fn fills_conserve_liquidity_for_arbitrary_books() {
	let mut rng = rand::rngs::StdRng::from_seed([11u8; 32]);

	for _ in 0..256 {
		let mut pool_state = PoolState::new();
		let mut minted = Amount::zero();
		// Start anywhere in the valid range rather than always at zero
		let mut tick: Tick = rng.gen_range(MIN_TICK..(MAX_TICK - 4 * 600));

		// Order sizes span whole orders of magnitude
		let scale = [
			Amount::from(1u128),
			Amount::from(2u128),
			Amount::from(5u128),
			Amount::from(100u128),
			Amount::from(1_000_000u128),
			Amount::from(1_000_000_000u128),
			Amount::from(u128::MAX),
			// Divided so that a whole book of these still fits a `U256`.
			Amount::MAX / 128,
		][rng.gen_range(0..8)];

		for _ in 0..rng.gen_range(1..5) {
			// Distinct ids at a price, so no two orders merge into one position.
			for id in 0..rng.gen_range(1u16..20) {
				let amount = random_amount(&mut rng, scale);
				assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id), tick, amount));
				minted += amount;
			}
			tick += rng.gen_range(1..600);
		}

		// Half the time a swap that could take the book several times over, half the time one too
		// small to give every order a whole unit.
		let swapped = if rng.gen() {
			random_amount(
				&mut rng,
				minted.saturating_mul(Amount::from(2u128)).max(Amount::from(2u128)),
			)
		} else {
			Amount::from(rng.gen_range(1u128..=16u128))
		};
		let (_output, _remaining, fills) = swap::<BaseToQuote>(&mut pool_state, swapped, None);

		let sold = total_of(&fills, |fill| fill.sold_amount);
		let left = pool_state
			.liquidity::<BaseToQuote>()
			.into_iter()
			.fold(Amount::zero(), |total, (_, amount)| total + amount);
		assert_eq!(sold + left, minted, "every unit is either still on the book or was bought");

		for fill in &fills {
			match pool_state.position::<BaseToQuote>(&fill.lp, fill.tick) {
				Some(position) => assert_eq!(position.amount, fill.remaining_amount),
				None => assert!(fill.remaining_amount.is_zero(), "a live order was dropped"),
			}
		}
	}
}

// A swap too small to give every order a whole unit still has to be filled exactly. Shares are
// floored, so an order whose exact share falls below one unit gets nothing; but because each share
// is taken against what is *left* to distribute rather than against the total, whatever that order
// didn't take raises the share of the orders behind it. One order misses out and the rest give a
// whole unit each.
//
// Flooring against the total would instead round all 100 shares to zero, leaving the entire swap
// for whichever order absorbed the remainder — far more than the single unit it holds.
#[test]
fn can_handle_zero_share_order_fill() {
	let tick = 0;
	let order_count = 100u16;

	let mut pool_state = PoolState::new();
	for id in 0..order_count {
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id), tick, 1.into()));
	}

	// One less than the liquidity available, so a share floored against the total would be zero
	// for every order.
	let swapped = Amount::from(order_count - 1);
	let (output, remaining, fills) = swap::<BaseToQuote>(&mut pool_state, swapped, None);

	assert_eq!(output, swapped);
	assert_eq!(remaining, Amount::zero());
	// We expect 99 fills of 1 unit each. One order misses out. No 0 amount fill is created.
	assert_eq!(fills.len(), (order_count - 1) as usize);
	for fill in &fills {
		assert_eq!(fill.sold_amount, 1.into());
	}
	assert_eq!(pool_state.liquidity::<BaseToQuote>(), vec![(tick, 1.into())]);
}

/// An order bought in its entirety is dropped, and a price with no orders left stops being quoted.
#[test]
fn filled_orders_are_removed() {
	let (near, far) = (0, 120);
	let mut pool_state = PoolState::new();
	assert_ok!(pool_state.mint::<QuoteToBase>(&lp(0), near, 1000.into()));
	assert_ok!(pool_state.mint::<QuoteToBase>(&lp(1), near, 1000.into()));
	assert_ok!(pool_state.mint::<QuoteToBase>(&lp(2), far, 1000.into()));

	// Enough to take the whole of the nearest price and nothing else.
	let (_output, _remaining, fills) = swap::<QuoteToBase>(&mut pool_state, 2000.into(), None);

	assert_eq!(fills.len(), 2);
	assert!(fills.iter().all(|fill| fill.remaining_amount.is_zero() && fill.tick == near));

	assert_eq!(pool_state.position::<QuoteToBase>(&lp(0), near), None);
	assert_eq!(pool_state.position::<QuoteToBase>(&lp(1), near), None);
	assert_eq!(pool_state.liquidity::<QuoteToBase>(), vec![(far, 1000.into())]);
	assert_eq!(pool_state.current_sqrt_price::<QuoteToBase>(), Some(SqrtPrice::from_tick(far)));
}

// Regression test: a limit order placed at the extreme boundary tick must still be matched by a
// swap with no price limit.
#[test]
fn boundary_tick_limit_order_consumed_without_price_limit() {
	for tick in [MIN_TICK, MAX_TICK] {
		let mut pool_state = PoolState::new();
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(0), tick, 1000.into()));
		assert_eq!(
			swap::<BaseToQuote>(&mut pool_state, Amount::MAX, None).0,
			1000.into(),
			"limit order at tick {tick} should be fully consumed by an unbounded swap"
		);
	}
}

#[cfg(feature = "slow-tests")]
#[test]
fn every_price_in_the_range_can_be_swapped_out() {
	// A realistic ceiling for one price, and low enough that the totals stay below the point
	// where the conversions saturate.
	let liquidity_per_price = Amount::from(u128::MAX);

	let mut pool_state = PoolState::new();

	for tick in MIN_TICK..=MAX_TICK {
		assert_eq!(
			pool_state.mint::<BaseToQuote>(&lp(0), tick, liquidity_per_price).unwrap(),
			Position::new(liquidity_per_price)
		);
	}

	assert_eq!(
		liquidity_per_price * (1 + MAX_TICK - MIN_TICK),
		std::iter::repeat_with(|| {
			pool_state
				.swap::<BaseToQuote>(Amount::MAX, None, 0, &mut FillQuota::Unlimited, None)
				.report
				.outcome
				.output_amount
		})
		.take_while(|x| !x.is_zero())
		.fold(Amount::zero(), |acc, x| acc + x)
	);
}

// Zero-amount orders are unreachable through minting, but malformed liquidity must not cause
// division by zero, and distributing zero on both sides must leave live orders untouched.
#[test]
fn fill_orders_handles_zero_amounts() {
	let sqrt_price = SqrtPrice::from_tick(0);
	let tick = sqrt_price.to_tick();
	let fill_orders = |orders: &mut Orders<LiquidityProvider>, sold: u128, bought: u128| {
		let mut fills = Vec::new();
		let participants = all_participating(orders);
		assert_eq!(
			super::fill_orders(
				orders,
				sqrt_price,
				&participants,
				sold.into(),
				bought.into(),
				&mut fills
			),
			Amount::zero()
		);
		fills
	};

	// A zero-amount order on its own: nothing to fill, and the order is dropped.
	let mut orders = Orders::from([(lp(0), Position::new(Amount::zero()))]);
	assert!(fill_orders(&mut orders, 0, 0).is_empty());
	assert!(orders.is_empty());

	// A zero-amount order alongside a real one must not affect what the real one is filled for.
	let mut orders =
		Orders::from([(lp(0), Position::new(Amount::zero())), (lp(1), Position::new(1000.into()))]);
	assert_eq!(
		fill_orders(&mut orders, 400, 800),
		vec![Fill {
			lp: lp(1),
			tick,
			sold_amount: 400.into(),
			bought_amount: 800.into(),
			remaining_amount: 600.into(),
		}]
	);
	assert_eq!(
		orders,
		Orders::from([(lp(1), Position { amount: 600.into(), original_amount: 1000.into() })])
	);

	// The same, with the zero-amount order visited after the remaining liquidity has run out.
	let mut orders =
		Orders::from([(lp(0), Position::new(1000.into())), (lp(1), Position::new(Amount::zero()))]);
	assert_eq!(
		fill_orders(&mut orders, 1000, 2000),
		vec![Fill {
			lp: lp(0),
			tick,
			sold_amount: 1000.into(),
			bought_amount: 2000.into(),
			remaining_amount: Amount::zero(),
		}]
	);
	assert!(orders.is_empty());

	// A zero-amount fill leaves the orders as they were, and reports nothing.
	let mut orders = Orders::from([(lp(0), Position::new(1000.into()))]);
	assert!(fill_orders(&mut orders, 0, 0).is_empty());
	assert_eq!(orders, Orders::from([(lp(0), Position::new(1000.into()))]));
}

mod fill_proptests {
	use super::*;
	use proptest::prelude::*;
	use std::collections::BTreeSet;

	/// Order sizes spanning what matters: dust that floors to nothing, ordinary amounts, and sizes
	/// large enough for the share arithmetic to leave `u128`. Capped so a whole book fits a `U256`.
	fn amount() -> impl Strategy<Value = Amount> {
		prop_oneof![
			4 => (1..=16u128).prop_map(Amount::from),
			3 => (1..=1_000_000_000u128).prop_map(Amount::from),
			2 => (1..=u128::MAX).prop_map(Amount::from),
			1 => (any::<u128>(), any::<u128>())
				.prop_map(|(hi, lo)| ((Amount::from(hi) << 128) | Amount::from(lo)) >> 5)
				.prop_map(|amount| amount.max(Amount::one())),
		]
	}

	/// Anything from nothing to the whole book, with the edges and tiny amounts over-represented.
	fn sold_amount(available: Amount) -> impl Strategy<Value = Amount> {
		prop_oneof![
			Just(Amount::zero()),
			Just(available),
			(1..=16u128).prop_map(move |sold| Amount::from(sold).min(available)),
			any::<u128>().prop_map(move |fraction| {
				mul_div_floor(available, fraction.into(), u128::MAX.into())
			}),
		]
	}

	fn fill_case() -> impl Strategy<Value = (Vec<Amount>, Amount, Amount)> {
		prop::collection::vec(amount(), 1..=20).prop_flat_map(|amounts| {
			let available = amounts
				.iter()
				.fold(Amount::zero(), |total, amount| total.saturating_add(*amount));
			(Just(amounts), sold_amount(available), prop_oneof![Just(Amount::zero()), amount()])
		})
	}

	proptest! {
		#![proptest_config(ProptestConfig { cases: 2048, ..Default::default() })]

		/// Allocations conserve assets and obey the supplied fill rate's rounding bounds. The
		/// independently generated totals need not correspond to the tick's limit price.
		#[test]
		fn fills_conserve_assets_and_bound_rounding_at_the_fill_rate((amounts, sold, bought) in fill_case()) {
			let sqrt_price = SqrtPrice::from_tick(0);
			let before = amounts
				.iter()
				.enumerate()
				.map(|(id, amount)| (lp(id as u16), Position::new(*amount)))
				.collect::<Orders<LiquidityProvider>>();
			let participants = all_participating(&before);

			let mut orders = before.clone();
			let mut fills = Vec::new();
			let dust =
				fill_orders(&mut orders, sqrt_price, &participants, sold, bought, &mut fills);

			// All sold liquidity reaches the swap; consumed input funds LP proceeds and surplus.
			prop_assert_eq!(total_of(&fills, |fill| fill.sold_amount), sold);
			prop_assert_eq!(total_of(&fills, |fill| fill.bought_amount) + dust, bought);

			// At most one fill per order, largest first and in book order among equals.
			let order_of =
				|fill: &Fill<LiquidityProvider>| (core::cmp::Reverse(before[&fill.lp].amount), fill.lp.clone());
			prop_assert!(fills.windows(2).all(|pair| order_of(&pair[0]) < order_of(&pair[1])));

			let mut filled = BTreeSet::new();
			for fill in &fills {
				prop_assert!(!fill.sold_amount.is_zero());
				let position = before.get(&fill.lp).unwrap();
				prop_assert!(fill.sold_amount <= position.amount);
				prop_assert_eq!(fill.remaining_amount, position.amount - fill.sold_amount);
				match orders.get(&fill.lp) {
					Some(after) => {
						prop_assert_eq!(after.amount, fill.remaining_amount);
						prop_assert_eq!(after.original_amount, position.original_amount);
					},
					None => prop_assert!(fill.remaining_amount.is_zero(), "a live order was dropped"),
				}
				filled.insert(fill.lp.clone());
			}

			// Orders that got nothing are left exactly as they were.
			for (lp, position) in before.iter() {
				if !filled.contains(lp) {
					prop_assert_eq!(orders.get(lp), Some(position));
				}
			}

			if sold.is_zero() {
				// Cross-multiplying by zero cannot constrain the payout policy.
				prop_assert_eq!(&orders, &before);
				prop_assert!(fills.is_empty());
				prop_assert_eq!(dust, bought);
			} else {
				prop_assert!(dust < Amount::from(before.len()));
				// Every order, including the last, loses less than one proceeds unit.
				for fill in &fills {
					let paid = fill.bought_amount.full_mul(sold);
					let owed = fill.sold_amount.full_mul(bought);
					prop_assert!(paid <= owed);
					prop_assert!(owed - paid < U512::from(sold));
				}
			}
		}
	}
}

/// A swap that will fill orders normally until its unfiltered fills run out.
fn quota() -> FillQuota {
	FillQuota::bounded()
}

/// Every one of a swap's unfiltered fills goes to whatever it finds, however far too small to get
/// past the filter, and only after the last of them does filtering start.
#[test]
fn filtering_starts_after_the_last_unfiltered_fill() {
	let mut pool_state = PoolState::new();
	// One more piece of dust at the best price than there are unfiltered fills, all of it far too
	// small to get past the filter, and one order behind it to make up the rest.
	for id in 0..=UNFILTERED_FILLS_PER_SWAP as u16 {
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id), 1, 1.into()));
	}
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(u16::MAX), 0, 1_000_000.into()));
	let report = pool_state
		.swap::<BaseToQuote>(500_000.into(), None, 0, &mut quota(), None)
		.report;
	// Every unfiltered fill goes to dust, the dust left after them is passed over, and the order
	// behind is filled.
	assert_eq!(report.outcome.limit_order_fills.len(), UNFILTERED_FILLS_PER_SWAP as usize + 1);
	assert_eq!(report.excluded_limit_order_count, 1);
}

/// Once its unfiltered fills are gone the swap raises what an order must hold to be worth
/// filling, so a wall of small orders is stepped over rather than spamming the fills.
#[test]
fn a_wall_of_small_orders_is_stepped_over() {
	let wall = MAX_FILLS_PER_SWAP as u16;
	let mut pool_state = PoolState::new();
	// The wall holds the best price, ahead of the liquidity the swap is really after.
	for id in 0..wall {
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id), 1, 1.into()));
	}
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(wall), 0, 1_000_000.into()));

	let (output, remaining, fills) =
		swap_with_fill_quota::<BaseToQuote>(&mut pool_state, 1_000_000.into(), None, quota());

	assert!(remaining.is_zero(), "the swap must get through the wall, not stall on it");
	assert!(output >= 999_000.into(), "and reach the liquidity behind it, got {output}");
	// The wall is bought into rather than ignored, so walling the book off is not free.
	assert!(fills.len() <= MAX_FILLS_PER_SWAP as usize);
}

/// However the book is filled, one swap never settles more orders than its quota allows.
#[test]
fn a_swap_never_fills_more_orders_than_its_quota() {
	let mut rng = rand::rngs::StdRng::from_seed([3u8; 32]);
	let mut pool_state = PoolState::new();
	for id in 0..(MAX_FILLS_PER_SWAP as u16) {
		for tick in [6930, 6931, 6932] {
			assert_ok!(pool_state.mint::<BaseToQuote>(
				&lp(id),
				tick,
				random_amount(&mut rng, 1000.into())
			));
		}
	}

	for bought in [1u32, 1000, 100_000, 10_000_000, u32::MAX] {
		let mut pool_state = pool_state.clone();
		let (_output, _remaining, fills) =
			swap_with_fill_quota::<BaseToQuote>(&mut pool_state, bought.into(), None, quota());

		assert!(
			fills.len() <= MAX_FILLS_PER_SWAP as usize,
			"{} fills buying {bought}",
			fills.len(),
		);
		// At most one fill per order, so no order is paid out twice in one swap.
		let distinct = fills
			.iter()
			.map(|fill| (&fill.lp, fill.tick))
			.collect::<std::collections::BTreeSet<_>>();
		assert_eq!(distinct.len(), fills.len());
	}
}

/// A swap that gets everything it asked for has passed nothing over, however many orders the
/// book still holds behind where it stopped.
#[test]
fn a_satisfied_swap_reports_nothing_excluded() {
	let mut pool_state = PoolState::new();
	for tick in [0, 1, 2] {
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(tick as u16), tick, 1000.into()));
	}

	// Buys out the best price and stops, leaving the two worse ones untouched.
	let mut quota = FillQuota::bounded();
	let outcome = pool_state.swap::<BaseToQuote>(1000.into(), None, 0, &mut quota, None);

	assert!(outcome.report.remaining_input_amount.is_zero(), "the swap must be satisfied");
	assert_eq!(outcome.report.excluded_limit_order_count, 0);
}

/// A swap the limit stops short of what it asked for reports the orders it passed over, so that
/// falling short of liquidity it could not reach is distinguishable from liquidity not being
/// there.
#[test]
fn a_swap_stopped_by_the_limit_reports_what_it_passed_over() {
	let count = MAX_FILLS_PER_SWAP as u16 + 5;
	let mut pool_state = PoolState::new();
	for id in 0..count {
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id), id as Tick, 1000.into()));
	}

	let mut quota = FillQuota::bounded();
	let outcome = pool_state.swap::<BaseToQuote>(
		Amount::from(count) * Amount::from(2000u32),
		None,
		0,
		&mut quota,
		None,
	);

	assert!(!outcome.report.remaining_input_amount.is_zero(), "the swap must fall short");
	assert!(
		outcome.report.excluded_limit_order_count > 0,
		"orders left unreachable must be reported, or this looks like an empty pool",
	);
	// The bar turns the swap away before its fills run out: needing more orders than it has
	// fills is exactly what the bar detects, so it never gets as far as spending them all.
	assert!(outcome.report.outcome.limit_order_fills.len() < MAX_FILLS_PER_SWAP as usize);
}

/// The quota's first fills go to any order. From then on an order has to sell what each of the
/// fills left would have to sell to finish the swap, and once there is no fill or nothing left to
/// buy, none can be given at all.
#[test]
fn fill_quota_minimum_amount() {
	// A quota that has already given out `used` fills.
	let after = |used: u32| {
		let mut quota = quota();
		quota.take(used);
		quota
	};
	let filtered_from = UNFILTERED_FILLS_PER_SWAP;
	let fills_left = MAX_FILLS_PER_SWAP - filtered_from;

	// Unfiltered, whatever is left to buy.
	for used in [0, filtered_from - 1] {
		assert_eq!(after(used).minimum_amount(1_000.into()), Some(Amount::zero()));
		assert_eq!(after(used).minimum_amount(Amount::zero()), Some(Amount::zero()));
	}

	// Filtered
	// x fills left to buy 10x, so each fill must sell 10.
	assert_eq!(
		after(filtered_from).minimum_amount(Amount::from(fills_left) * Amount::from(10)),
		Some(10.into())
	);
	// x fills left to buy 10x + 1, so each needs to sell a little over 10, rounding up to 11.
	assert_eq!(
		after(filtered_from).minimum_amount(Amount::from(fills_left) * Amount::from(10) + 1),
		Some(11.into())
	);
	// x fills left to buy x-1, so each needs to sell around 0.9, rounding up to 1.
	assert_eq!(after(filtered_from).minimum_amount(Amount::from(fills_left - 1)), Some(1.into()));
	// 1 fill left to buy 7, so it must sell 7.
	assert_eq!(after(MAX_FILLS_PER_SWAP - 1).minimum_amount(7.into()), Some(7.into()));

	// No fill left to give.
	assert_eq!(after(MAX_FILLS_PER_SWAP).minimum_amount(1_000.into()), None);
	// Nothing left to buy: the swap ends at this price, so any order may share the fill.
	assert_eq!(after(filtered_from).minimum_amount(Amount::zero()), Some(Amount::zero()));

	// With no limit, every order will do.
	assert_eq!(FillQuota::Unlimited.minimum_amount(Amount::zero()), Some(Amount::zero()));
}

/// Taking fills reports when the quota has none left, and never with no limit.
#[test]
fn fill_quota_take() {
	let mut quota = quota();
	assert_eq!(quota.remaining(), MAX_FILLS_PER_SWAP);
	assert!(!quota.take(MAX_FILLS_PER_SWAP - 1));
	assert!(quota.take(1));
	assert_eq!(quota.remaining(), 0);
	// Taking more than is left does not wrap.
	assert!(quota.take(5));
	assert_eq!(quota.remaining(), 0);

	let mut unlimited = FillQuota::Unlimited;
	assert!(!unlimited.take(u32::MAX));
	assert_eq!(unlimited.remaining(), u32::MAX);
}

/// A quota that has used its unfiltered fills, with `fills_left` remaining.
fn filtered_quota(fills_left: u32) -> FillQuota {
	assert!(fills_left <= MAX_FILLS_PER_SWAP - UNFILTERED_FILLS_PER_SWAP);
	FillQuota::Remaining(fills_left)
}

fn book(amounts: &[u32]) -> Orders<LiquidityProvider> {
	amounts
		.iter()
		.enumerate()
		.map(|(id, amount)| (lp(id as u16), Position::new((*amount).into())))
		.collect()
}

/// A price whose largest order cannot clear the quota's minimum for the first
/// fill, is passed over without anything being chosen.
#[test]
fn select_passes_over_a_price_it_cannot_use() {
	assert!(Participants::select(&book(&[]), &quota(), 1_000.into()).is_none());

	// 750 fills left to buy 750,000, so the first fill has to sell 1,000.
	let quota = filtered_quota(750);
	assert!(Participants::select(&book(&[999, 999, 500]), &quota, 750_000.into()).is_none());

	// Try the next price with a single order that can clear the bar, and it is chosen.
	let chosen = Participants::select(&book(&[999, 1_000, 500]), &quota, 750_000.into()).unwrap();
	assert_eq!(chosen.orders, vec![lp(1)]);
	assert_eq!(chosen.available, 1_000.into());
}

/// Once the orders taken cover what the swap still wants, this is its last price, so it takes
/// every other order it has a fill for and shares the fill between them all, rather than leaving
/// the largest to take the whole of it.
#[test]
fn select_shares_the_last_price_between_every_order_it_has_fills_for() {
	// Every order clears the bar, which starts at 100,000 / 750 = 133 and only falls.
	let chosen =
		Participants::select(&book(&[200; 700]), &filtered_quota(750), 100_000.into()).unwrap();
	assert_eq!(chosen.count(), 700);

	// With fewer fills than orders, as many as there are fills.
	let chosen =
		Participants::select(&book(&[200; 700]), &filtered_quota(600), 100_000.into()).unwrap();
	assert_eq!(chosen.count(), 600);
}

/// The chosen orders are largest first, and among orders the same size the one the book stores
/// first wins, so which of them are chosen does not depend on how the sort breaks ties.
#[test]
fn select_takes_the_largest_and_breaks_ties_by_book_order() {
	let chosen =
		Participants::select(&book(&[100, 50, 100, 300, 100, 100]), &filtered_quota(3), 500.into())
			.unwrap();
	// The largest is fist followed by the two 100s, leaving out the other 100 and the 50.
	assert_eq!(chosen.orders, vec![lp(3), lp(0), lp(2)]);
	assert_eq!(chosen.available, 500.into());
}

/// With no limit, every order at the price takes part.
#[test]
fn select_with_no_limit_takes_everything() {
	let chosen =
		Participants::select(&book(&[5, 1, 9, 3]), &FillQuota::Unlimited, Amount::zero()).unwrap();
	assert_eq!(chosen.orders, vec![lp(2), lp(0), lp(3), lp(1)]);
	assert_eq!(chosen.available, 18.into());
}

/// Splitting an order into many smaller ones does not win it a bigger share of a fill once the
/// swap is filtering: every order that clears the bar shares the fill in proportion to what it
/// holds, so the pieces between them get exactly what the whole order would have.
#[test]
fn splitting_an_order_does_not_win_a_bigger_share() {
	fn sold_by_b(pieces: u32) -> Amount {
		let mut pool_state = PoolState::new();
		// Dust at a better price uses up the unfiltered fills first.
		for id in 0..UNFILTERED_FILLS_PER_SWAP as u16 {
			assert_ok!(pool_state.mint::<BaseToQuote>(&lp(10_000 + id), 1, 1.into()));
		}
		assert_ok!(pool_state.mint::<BaseToQuote>(&lp(0), 0, 1_000.into()));
		for id in 0..pieces {
			assert_ok!(pool_state.mint::<BaseToQuote>(
				&lp(1 + id as u16),
				0,
				(1_000 / pieces).into()
			));
		}
		let (_, _, fills) =
			swap_with_fill_quota::<BaseToQuote>(&mut pool_state, 1_500.into(), None, quota());
		fills
			.iter()
			.filter(|fill| fill.tick == 0 && fill.lp != lp(0))
			.fold(Amount::zero(), |total, fill| total + fill.sold_amount)
	}

	let whole = sold_by_b(1);
	for pieces in [10, 100] {
		assert_eq!(sold_by_b(pieces), whole, "split {pieces} ways");
	}
}

mod fill_limit_proptests {
	use super::*;
	use proptest::prelude::*;

	/// A book of orders across a handful of prices, mixing dust with ordinary sizes, big enough to
	/// run past the limit.
	fn book() -> impl Strategy<Value = Vec<(Tick, Amount)>> {
		prop::collection::vec(
			(-20..=20i32, prop_oneof![1u32..=10, 1u32..=10_000].prop_map(Amount::from)),
			1..=1_500,
		)
	}

	fn pool_of(book: &[(Tick, Amount)]) -> PoolState {
		let mut pool_state = PoolState::new();
		for (id, (tick, amount)) in book.iter().enumerate() {
			assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id as u16), *tick, *amount));
		}
		pool_state
	}

	proptest! {
		#![proptest_config(ProptestConfig { cases: 48, ..Default::default() })]

		/// However the book is laid out, the limit bounds what a swap settles, fills each order
		/// at most once, takes prices best first, and never leaves the swapper better off than no
		/// limit would, beyond rounding.
		#[test]
		fn the_fill_limit_holds_for_any_book(book in book(), input in 1u32..=20_000_000) {
			let mut limited = pool_of(&book);
			let mut unlimited = limited.clone();

			let (output, _, fills) =
				swap_with_fill_quota::<BaseToQuote>(&mut limited, input.into(), None, quota());
			let (unlimited_output, _, _) = swap::<BaseToQuote>(&mut unlimited, input.into(), None);

			prop_assert!(fills.len() <= MAX_FILLS_PER_SWAP as usize);

			let distinct = fills
				.iter()
				.map(|fill| (&fill.lp, fill.tick))
				.collect::<std::collections::BTreeSet<_>>();
			prop_assert_eq!(distinct.len(), fills.len());

			// Higher ticks are the better price for this direction.
			prop_assert!(fills.windows(2).all(|pair| pair[0].tick >= pair[1].tick));

			// Buying out a price rounds what the swap pays for it up, so a swap that buys out
			// fewer prices can come out a unit or so ahead per price. Prices here are near one, so
			// a unit of input is about a unit of output.
			let prices = book.iter().map(|(tick, _)| tick).collect::<std::collections::BTreeSet<_>>();
			prop_assert!(
				output <= unlimited_output + Amount::from(2 * prices.len()),
				"limited {} vs unlimited {}",
				output,
				unlimited_output
			);
		}

		/// A book with no more orders than a swap fills unfiltered is swapped against exactly as
		/// it would be with no limit at all.
		#[test]
		fn a_small_book_is_unaffected_by_the_limit(
			book in prop::collection::vec(
				(-20..=20i32, (1u32..=10_000).prop_map(Amount::from)),
				1..=UNFILTERED_FILLS_PER_SWAP as usize,
			),
			input in 1u32..=20_000_000,
		) {
			let mut limited = pool_of(&book);
			let mut unlimited = limited.clone();

			prop_assert_eq!(
				swap_with_fill_quota::<BaseToQuote>(&mut limited, input.into(), None, quota()),
				swap::<BaseToQuote>(&mut unlimited, input.into(), None)
			);
			prop_assert_eq!(limited, unlimited);
		}
	}
}
