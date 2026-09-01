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
use cf_amm_math::{MAX_SQRT_PRICE, MAX_TICK, MIN_SQRT_PRICE, MIN_TICK};

use super::*;

use cf_utilities::{assert_matches, assert_ok};

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

fn lp(id: u8) -> LiquidityProvider {
	LiquidityProvider::from([id; 32])
}

fn total_of(
	fills: &[Fill<LiquidityProvider>],
	f: impl Fn(&Fill<LiquidityProvider>) -> Amount,
) -> Amount {
	fills.iter().fold(Amount::zero(), |total, fill| total + f(fill))
}

/// Performs a swap, checking the invariants.
fn swap<SD: SwapDirection>(
	pool_state: &mut PoolState,
	amount: Amount,
	sqrt_price_limit: Option<SqrtPrice>,
) -> (Amount, Amount, Vec<Fill<LiquidityProvider>>) {
	let (output_amount, remaining_amount, fills) =
		pool_state.swap::<SD>(amount, sqrt_price_limit, 0);

	assert_eq!(
		total_of(&fills, |fill| fill.sold_amount),
		output_amount,
		"the orders must give up exactly what the swap bought"
	);
	assert_eq!(
		total_of(&fills, |fill| fill.bought_amount),
		amount - remaining_amount,
		"the orders must be owed exactly what the swap paid in"
	);

	(output_amount, remaining_amount, fills)
}

/// The amounts used as parameters to input/output amount function are
/// guaranteed to be <= MAX_LIQUIDITY_PER_PRICE. This test checks that MAX_LIQUIDITY_PER_PRICE is
/// set low enough that those calculations don't overflow.
#[test]
fn max_liquidity() {
	fn checks<SD: SwapDirection>(price: Price) {
		// Overflow will return None
		assert!(SD::input_amount_floor(MAX_LIQUIDITY_PER_PRICE, price).is_some());
		assert!(SD::input_amount_ceil(MAX_LIQUIDITY_PER_PRICE, price).is_some());
		assert!(SD::output_amount_floor(MAX_LIQUIDITY_PER_PRICE, price).is_some());
		assert!(SD::output_amount_ceil(MAX_LIQUIDITY_PER_PRICE, price).is_some());
	}

	for price in [MIN_SQRT_PRICE, MAX_SQRT_PRICE].map(Price::from) {
		checks::<BaseToQuote>(price);
		checks::<QuoteToBase>(price);
	}
}

#[test]
fn mint() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		for good in [MIN_TICK, MAX_TICK] {
			let mut pool_state = PoolState::new();
			assert_eq!(
				assert_ok!(pool_state.mint::<SD>(&lp(0), good, 1000.into())),
				PositionInfo::new(1000.into())
			);
		}

		for bad in [MIN_TICK - 1, MAX_TICK + 1] {
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.mint::<SD>(&lp(0), bad, 1000.into()),
				Err(PositionError::InvalidTick)
			);
		}

		for good in [MAX_LIQUIDITY_PER_PRICE, MAX_LIQUIDITY_PER_PRICE - 1, 1.into()] {
			let mut pool_state = PoolState::new();
			assert_eq!(assert_ok!(pool_state.mint::<SD>(&lp(0), 0, good)), PositionInfo::new(good));
		}

		for bad in [MAX_LIQUIDITY_PER_PRICE + 1, MAX_LIQUIDITY_PER_PRICE + 2] {
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.mint::<SD>(&lp(0), 0, bad),
				Err(PositionError::Other(MintError::MaximumLiquidity))
			);
		}

		// The maximum is per price, over every lp's orders combined.
		{
			let mut pool_state = PoolState::new();
			assert_ok!(pool_state.mint::<SD>(&lp(0), 0, MAX_LIQUIDITY_PER_PRICE - 1));
			assert_ok!(pool_state.mint::<SD>(&lp(1), 0, 1.into()));
			assert_matches!(
				pool_state.mint::<SD>(&lp(2), 0, 1.into()),
				Err(PositionError::Other(MintError::MaximumLiquidity))
			);
		}

		// A failed mint leaves nothing behind.
		{
			let mut pool_state = PoolState::new();
			assert_matches!(
				pool_state.mint::<SD>(&lp(0), 0, MAX_LIQUIDITY_PER_PRICE + 1),
				Err(PositionError::Other(MintError::MaximumLiquidity))
			);
			assert_matches!(pool_state.position::<SD>(&lp(0), 0), Err(PositionError::NonExistent));
			assert!(pool_state.liquidity::<SD>().is_empty());
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
				PositionInfo::new(1000.into())
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
				PositionInfo::new(amount)
			);
			assert_eq!(
				assert_ok!(pool_state.burn::<SD>(&lp(0), tick, amount)),
				(amount, PositionInfo::default())
			);
			// Burning an order in its entirety removes it.
			assert_matches!(
				pool_state.position::<SD>(&lp(0), tick),
				Err(PositionError::NonExistent)
			);
		}
		{
			// Burning one lp's order leaves the others at that price alone.
			let mut pool_state = PoolState::new();
			let tick = 120;
			let amount = U256::from(1000);
			assert_ok!(pool_state.mint::<SD>(&lp(1), tick, 56.into()));
			assert_eq!(
				assert_ok!(pool_state.mint::<SD>(&lp(0), tick, amount)),
				PositionInfo::new(amount)
			);
			assert_ok!(pool_state.mint::<SD>(&lp(2), tick, 16.into()));
			assert_eq!(
				assert_ok!(pool_state.burn::<SD>(&lp(0), tick, amount)),
				(amount, PositionInfo::default())
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
				(amount - swapped, PositionInfo::default())
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
	let tick = 0;
	let mut pool_state = PoolState::new();
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(0), tick, 1000.into()));
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(1), tick, 2000.into()));
	assert_ok!(pool_state.mint::<BaseToQuote>(&lp(2), tick, 7000.into()));

	let (output, remaining, fills) = swap::<BaseToQuote>(&mut pool_state, 1000.into(), None);
	assert_eq!((output, remaining), (1000.into(), Amount::zero()));

	assert_eq!(
		fills,
		vec![
			Fill {
				lp: lp(0),
				tick,
				sold_amount: 100.into(),
				bought_amount: 100.into(),
				remaining_amount: 900.into(),
			},
			Fill {
				lp: lp(1),
				tick,
				sold_amount: 200.into(),
				bought_amount: 200.into(),
				remaining_amount: 1800.into(),
			},
			Fill {
				lp: lp(2),
				tick,
				sold_amount: 700.into(),
				bought_amount: 700.into(),
				remaining_amount: 6300.into(),
			},
		]
	);
}

/// The `swap` helper asserts that fills account for a swap exactly, but the books above are all
/// hand-picked round numbers. Distribution is arithmetic over arbitrary sizes, so drive arbitrary
/// ones through it: a unit conjured up is an lp paid for liquidity that never existed, and a unit
/// lost is an lp's liquidity vanishing off the book.
#[test]
fn fills_conserve_liquidity_for_arbitrary_books() {
	use rand::{Rng, SeedableRng};

	let mut rng = rand::rngs::StdRng::from_seed([11u8; 32]);

	for _ in 0..256 {
		let mut pool_state = PoolState::new();
		let mut minted = Amount::zero();
		let mut tick: Tick = 0;

		// Order sizes span whole orders of magnitude. Tiny ones matter most: they are what make a
		// share floor to zero, which is the case that decides whether an order that takes nothing
		// still gives up its claim on the rest.
		let scale = [1u128, 2, 5, 100, 1_000_000, 1_000_000_000][rng.gen_range(0..6)];

		for _ in 0..rng.gen_range(1..5) {
			// Distinct ids at a price, so no two orders merge into one position.
			for id in 0..rng.gen_range(1u8..20) {
				let amount = Amount::from(rng.gen_range(1u128..=scale));
				assert_ok!(pool_state.mint::<BaseToQuote>(&lp(id), tick, amount));
				minted += amount;
			}
			tick += rng.gen_range(1..600);
		}

		// Half the time a swap that could take the book several times over, half the time one too
		// small to give every order a whole unit.
		let swapped = Amount::from(if rng.gen() {
			rng.gen_range(1..=minted.as_u128().saturating_mul(2).max(2))
		} else {
			rng.gen_range(1..=16u128)
		});
		let (_output, _remaining, fills) = swap::<BaseToQuote>(&mut pool_state, swapped, None);

		let sold = total_of(&fills, |fill| fill.sold_amount);
		let left = pool_state
			.liquidity::<BaseToQuote>()
			.into_iter()
			.fold(Amount::zero(), |total, (_, amount)| total + amount);
		assert_eq!(sold + left, minted, "every unit is either still on the book or was bought");

		for fill in &fills {
			match pool_state.position::<BaseToQuote>(&fill.lp, fill.tick) {
				Ok(position) => assert_eq!(position.amount, fill.remaining_amount),
				Err(PositionError::NonExistent) =>
					assert!(fill.remaining_amount.is_zero(), "a live order was dropped"),
				Err(error) => panic!("unexpected {error:?}"),
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
	let order_count = 100u8;

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

	assert_matches!(
		pool_state.position::<QuoteToBase>(&lp(0), near),
		Err(PositionError::NonExistent)
	);
	assert_matches!(
		pool_state.position::<QuoteToBase>(&lp(1), near),
		Err(PositionError::NonExistent)
	);
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
fn maximum_liquidity_swap() {
	let mut pool_state = PoolState::new();

	for tick in MIN_TICK..=MAX_TICK {
		assert_eq!(
			pool_state.mint::<BaseToQuote>(&lp(0), tick, MAX_LIQUIDITY_PER_PRICE).unwrap(),
			PositionInfo::new(MAX_LIQUIDITY_PER_PRICE)
		);
	}

	assert_eq!(
		MAX_LIQUIDITY_PER_PRICE * (1 + MAX_TICK - MIN_TICK),
		std::iter::repeat_with(|| { pool_state.swap::<BaseToQuote>(Amount::MAX, None, 0).0 })
			.take_while(|x| !x.is_zero())
			.fold(Amount::zero(), |acc, x| acc + x)
	);
}
