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

#![cfg(test)]

use cf_utilities::assert_ok;
use core::convert::Infallible;
use sp_core::crypto::AccountId32;

use crate::range_orders::Liquidity;
use cf_amm_math::{Price, MAX_SQRT_PRICE, MAX_TICK, MIN_SQRT_PRICE};

use super::*;

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

/// Swaps as much of `amount` as the pool will take, returning the output and what is left.
fn swap(pool_state: &mut PoolState, order: Side, amount: Amount) -> (Amount, Amount) {
	let (SwapExecutionOutcome { output_amount, .. }, remaining_input_amount) =
		pool_state.swap_to_price(order, amount, PoolState::inner_worst_price(order));
	(output_amount, remaining_input_amount)
}

#[test]
fn test_basic_swaps() {
	fn inner(order: Side) {
		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(),
				range_orders: range_orders::PoolState::new(0, MIN_SQRT_PRICE).unwrap(),
			};

			assert_eq!(swap(&mut pool_state, order, 0.into()), (0.into(), 0.into()));
			assert_eq!(swap(&mut pool_state, order, Amount::MAX), (0.into(), Amount::MAX));
			assert_eq!(swap(&mut pool_state, order, 0.into()), (0.into(), 0.into()));
		}

		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(),
				range_orders: range_orders::PoolState::new(0, MIN_SQRT_PRICE).unwrap(),
			};

			let amount: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.mint_limit_order(
					&LiquidityProvider::from([0; 32]),
					!order,
					0,
					amount
				)),
				limit_orders::Position::new(amount)
			);

			assert_eq!(swap(&mut pool_state, order, 0.into()), (0.into(), 0.into()));
			assert_eq!(swap(&mut pool_state, order, Amount::MAX), (amount, Amount::MAX - amount));
		}

		{
			let initial_sqrt_price = match order.to_sold_pair() {
				Pairs::Base => MAX_SQRT_PRICE,
				Pairs::Quote => MIN_SQRT_PRICE,
			};
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(),
				range_orders: range_orders::PoolState::new(0, initial_sqrt_price).unwrap(),
			};

			let liquidity: range_orders::Liquidity = 10000;

			let (minted_amounts, minted_liquidity, collected_fees, position_info) =
				assert_ok!(pool_state.collect_and_mint_range_order(
					&LiquidityProvider::from([0; 32]),
					-100..100,
					range_orders::Size::Liquidity { liquidity },
					Result::<_, Infallible>::Ok
				));
			assert_eq!(minted_liquidity, liquidity);
			assert_eq!(
				collected_fees,
				range_orders::Collected {
					original_sqrt_price: initial_sqrt_price,
					..Default::default()
				}
			);
			assert_eq!(position_info.liquidity, liquidity);

			assert_eq!(swap(&mut pool_state, order, 0.into()), (0.into(), 0.into()));
			assert_eq!(
				swap(&mut pool_state, order, Amount::MAX),
				(
					minted_amounts[!order.to_sold_pair()] - 1, /* -1 is due to rounding down */
					Amount::MAX - minted_amounts[!order.to_sold_pair()]
				)
			);
		}

		{
			let initial_sqrt_price = match order.to_sold_pair() {
				Pairs::Base => MAX_SQRT_PRICE,
				Pairs::Quote => MIN_SQRT_PRICE,
			};
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(),
				range_orders: range_orders::PoolState::new(0, initial_sqrt_price).unwrap(),
			};

			let range_order_liquidity: Liquidity = 10000;

			let (range_order_minted_amounts, minted_liquidity, collected_fees, position_info) =
				assert_ok!(pool_state.collect_and_mint_range_order(
					&LiquidityProvider::from([0; 32]),
					-100..100,
					range_orders::Size::Liquidity { liquidity: range_order_liquidity },
					Result::<_, Infallible>::Ok
				));
			assert_eq!(minted_liquidity, range_order_liquidity);
			assert_eq!(
				collected_fees,
				range_orders::Collected {
					original_sqrt_price: initial_sqrt_price,
					..Default::default()
				}
			);
			assert_eq!(position_info.liquidity, range_order_liquidity);

			let limit_order_liquidity: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.mint_limit_order(
					&LiquidityProvider::from([0; 32]),
					!order,
					0,
					limit_order_liquidity
				)),
				limit_orders::Position::new(limit_order_liquidity)
			);

			assert_eq!(swap(&mut pool_state, order, 0.into()), (0.into(), 0.into()));
			assert_eq!(
				swap(&mut pool_state, order, Amount::MAX),
				(
					limit_order_liquidity + range_order_minted_amounts[!order.to_sold_pair()] - 1, /* -1 is due
					                                                                                * to rounding
					                                                                                * down */
					Amount::MAX -
						(limit_order_liquidity +
							range_order_minted_amounts[!order.to_sold_pair()]) -
						1 /* -1 is due to rounding down */
				)
			);
		}

		{
			let initial_sqrt_price = match order.to_sold_pair() {
				Pairs::Base => MAX_SQRT_PRICE,
				Pairs::Quote => MIN_SQRT_PRICE,
			};
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(),
				range_orders: range_orders::PoolState::new(0, initial_sqrt_price).unwrap(),
			};

			let mut mint_range_order = |lower_tick, upper_tick| {
				let liquidity = 100000;
				let (range_order_minted_amounts, minted_liquidity, collected_fees, position_info) =
					assert_ok!(pool_state.collect_and_mint_range_order(
						&LiquidityProvider::from([0; 32]),
						lower_tick..upper_tick,
						range_orders::Size::Liquidity { liquidity },
						Result::<_, Infallible>::Ok
					));
				assert_eq!(minted_liquidity, liquidity);
				assert_eq!(
					collected_fees,
					range_orders::Collected {
						original_sqrt_price: initial_sqrt_price,
						..Default::default()
					}
				);
				assert_eq!(position_info.liquidity, 100000);

				range_order_minted_amounts
			};
			let range_order_minted_amounts =
				mint_range_order(-100, -10) + mint_range_order(10, 100);

			let limit_order_liquidity: Amount = 10000.into();
			assert_eq!(
				assert_ok!(pool_state.mint_limit_order(
					&LiquidityProvider::from([0; 32]),
					!order,
					0,
					limit_order_liquidity
				)),
				limit_orders::Position::new(limit_order_liquidity)
			);

			assert_eq!(swap(&mut pool_state, order, 0.into()), (0.into(), 0.into()));
			assert_eq!(
				swap(&mut pool_state, order, Amount::MAX),
				(
					limit_order_liquidity + range_order_minted_amounts[!order.to_sold_pair()] - 2, /* -2 is due
					                                                                                * to rounding
					                                                                                * down */
					Amount::MAX -
						(limit_order_liquidity +
							range_order_minted_amounts[!order.to_sold_pair()])
				)
			);
		}
	}

	inner(Side::Sell);
	inner(Side::Buy);
}

// Test that we can correctly switch from executing limit orders to range orders
// and back:
#[test]
fn alternating_range_and_limit_orders() {
	use range_orders::Size;

	const LP: AccountId32 = AccountId32::new([1; 32]);
	const TICK_RANGE: core::ops::Range<i32> = -100..100;
	let limit_order_amount: Amount = 1_000_000.into();

	let mut pool = PoolState::new(500, Price::at_tick_zero()).unwrap();
	pool.mint_limit_order(&LP, Side::Buy, 0, limit_order_amount).unwrap();

	pool.collect_and_mint_range_order(
		&LP,
		TICK_RANGE,
		Size::Liquidity { liquidity: 1_000_000_000_000 },
		Result::<_, Infallible>::Ok,
	)
	.unwrap();

	pool.mint_limit_order(&LP, Side::Buy, 10, limit_order_amount).unwrap();

	assert_eq!(pool.limit_order(&LP, Side::Buy, 0).unwrap().amount, limit_order_amount);
	assert_eq!(pool.limit_order(&LP, Side::Buy, 10).unwrap().amount, limit_order_amount);
	assert_eq!(pool.range_order(&LP, TICK_RANGE).unwrap().0.fees.base, 0.into());

	let SwapExecutionOutcome { limit_order_fills, .. } =
		pool.swap(Side::Sell, 3_000_000.into()).unwrap();

	// Check that all three orders have been used in the swap:
	assert_eq!(limit_order_fills.len(), 2);
	assert!(limit_order_fills
		.iter()
		.all(|fill| fill.remaining_amount.is_zero() && fill.sold_amount == limit_order_amount));
	assert!(pool.range_order(&LP, TICK_RANGE).unwrap().0.fees.base > 0.into());
}

#[test]
fn limit_order_dust_is_accumulated_across_prices() {
	let mut pool = PoolState::new(0, Price::from_tick(69_082).unwrap()).unwrap();
	for tick in [69_082, 69_083] {
		for id in [0u8, 1] {
			pool.mint_limit_order(&AccountId32::new([id; 32]), Side::Sell, tick, 1.into())
				.unwrap();
		}
	}

	// Each tick sells two satoshis for 2,001 micro-USDC. Each LP receives 1,000.
	let outcome = pool.swap(Side::Buy, 4_002.into()).unwrap();
	assert_eq!(outcome.output_amount, 4.into());
	assert_eq!(outcome.limit_order_input_dust, 2.into());
	assert_eq!(outcome.limit_order_fills.len(), 4);
	assert!(outcome.limit_order_fills.iter().all(|fill| fill.bought_amount == 1_000.into()));
}

#[test]
fn check_price_adjustment_by_pool_fee() {
	use limit_orders::SwapDirection;
	use sp_core::U256;

	#[track_caller]
	fn test_case<SD: SwapDirection>(tick: i32, fee_hundredth_pips: u32) {
		let input = U256::from(100_000_000u32);

		let price = Price::from_tick(tick).unwrap();

		// Output is computed by reducing the input amount:
		let expected_output = {
			let input_minus_fees = reduce_by_pool_fee(input, fee_hundredth_pips);
			SD::output_amount_floor(input_minus_fees, price)
		};

		// Output is computed by adjusting the price instead:
		let output = {
			let sqrt_price = price.try_into_sqrt_price().unwrap();
			let adjusted_price =
				Price::from(sqrt_price_adjusted_by_pool_fee::<SD>(sqrt_price, fee_hundredth_pips));

			SD::output_amount_floor(input, adjusted_price)
		};

		// This shows that adjusting the price is equivalent to reducing the input amount:
		assert_eq!(output, expected_output);
	}

	for (tick, fee) in [(100, 500), (20_000, 500), (-20_000, 50_000), (-100, 0), (0, 0)] {
		test_case::<BaseToQuote>(tick, fee);
		test_case::<QuoteToBase>(tick, fee);
	}
}

// Near `MAX_TICK` the buy-side fee adjustment pushes the price past the representable tick range.
// It must be clamped to `MAX_SQRT_PRICE` (rather than rejected) so a swap can still execute against
// the order at the extreme price. The sell-side adjustment stays in range.
#[test]
fn fee_adjustment_clamps_out_of_range_buy_prices() {
	for (tick, fee) in [(887271, 100), (887267, 500), (MAX_TICK, 1), (MAX_TICK, 500_000)] {
		let sqrt_price = Price::from_tick(tick).unwrap().try_into_sqrt_price().unwrap();

		assert_eq!(
			sqrt_price_adjusted_by_pool_fee::<QuoteToBase>(sqrt_price, fee),
			MAX_SQRT_PRICE,
			"tick {tick} fee {fee}: buy-side adjusted price should clamp to MAX_SQRT_PRICE"
		);

		assert!(sqrt_price_adjusted_by_pool_fee::<BaseToQuote>(sqrt_price, fee) < MAX_SQRT_PRICE);
	}
}

/// A swap that keeps switching between limit and range orders makes several passes over the limit
/// orders, and all of them share the one quota. Without the limit the same swap fills far more.
#[test]
fn fill_limit_holds_across_switches_to_range_orders() {
	use range_orders::Size;

	const LP: AccountId32 = AccountId32::new([1; 32]);
	let mut pool = PoolState::new(0, Price::at_tick_zero()).unwrap();
	// Thin range liquidity, so its price moves through the limit orders a tick at a time.
	pool.collect_and_mint_range_order(
		&LP,
		-2000..2000,
		Size::Liquidity { liquidity: 20_000_000 },
		Result::<_, Infallible>::Ok,
	)
	.unwrap();
	let orders = limit_orders::MAX_FILLS_PER_SWAP as i32 + 500;
	for tick in -(orders / 2)..(orders / 2) {
		let mut key = [2u8; 32];
		key[0..4].copy_from_slice(&tick.to_be_bytes());
		pool.mint_limit_order(&AccountId32::new(key), Side::Buy, tick, 1_000.into())
			.unwrap();
	}

	let (unlimited, _) = pool.clone().swap_to_price(
		Side::Sell,
		3_000_000.into(),
		PoolState::inner_worst_price(Side::Sell),
	);
	assert!(
		unlimited.limit_order_fills.len() > limit_orders::MAX_FILLS_PER_SWAP as usize,
		"the book must be big enough for the limit to matter, got {}",
		unlimited.limit_order_fills.len(),
	);

	// Bounded, and allowed to fall short, so as to see how far it got.
	let limited = pool
		.inner_swap::<BaseToQuote>(3_000_000.into(), None, limit_orders::FillQuota::bounded())
		.outcome;
	assert!(limited.limit_order_fills.len() <= limit_orders::MAX_FILLS_PER_SWAP as usize);
	assert!(limited.output_amount <= unlimited.output_amount);
}

/// A limit order past the range orders' price is judged against the fill quota only once the
/// swap reaches it. The range orders spend input on the way there without using any fills, which
/// lowers the bar, so an order too small to fill at the start of the swap can be worth filling
/// by the time the swap gets to it.
#[test]
fn limit_orders_past_the_range_price_are_judged_when_reached() {
	use range_orders::Size;

	let lp = |id: u32| {
		let mut key = [0u8; 32];
		key[0..4].copy_from_slice(&id.to_be_bytes());
		AccountId32::new(key)
	};
	let small_order = lp(u32::MAX);

	let mut pool = PoolState::new(0, Price::at_tick_zero()).unwrap();
	// Takes about 10_000 to move from tick 0 to tick 50.
	pool.collect_and_mint_range_order(
		&lp(0),
		-10_000..10_000,
		Size::Liquidity { liquidity: 4_000_000 },
		Result::<_, Infallible>::Ok,
	)
	.unwrap();
	// Dust ahead of the range orders uses up the swap's unfiltered fills.
	for id in 1..=limit_orders::UNFILTERED_FILLS_PER_SWAP {
		pool.mint_limit_order(&lp(id), Side::Sell, -10, 1.into()).unwrap();
	}
	// Too small for a fill against the whole input, but not against what is left of it once the
	// range orders have taken it to tick 50.
	pool.mint_limit_order(&small_order, Side::Sell, 50, 5.into()).unwrap();
	pool.mint_limit_order(&lp(0), Side::Sell, 100, 1_000_000.into()).unwrap();

	let report =
		pool.inner_swap::<QuoteToBase>(12_000.into(), None, limit_orders::FillQuota::bounded());

	assert!(report.remaining_input_amount.is_zero());
	assert!(
		report.outcome.limit_order_fills.iter().any(|fill| fill.lp == small_order),
		"the order at tick 50 must be filled once the swap reaches it",
	);
	assert_eq!(report.excluded_limit_order_count, 0);
}
