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
use cf_amm_math::{Price, MAX_SQRT_PRICE, MIN_SQRT_PRICE};

use super::*;

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

#[test]
fn test_basic_swaps() {
	fn inner(order: Side) {
		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(0, MIN_SQRT_PRICE).unwrap(),
			};

			assert_eq!(pool_state.swap(order, 0.into(), None), (0.into(), 0.into()));
			assert_eq!(pool_state.swap(order, Amount::MAX, None), (0.into(), Amount::MAX));
			assert_eq!(pool_state.swap(order, 0.into(), None), (0.into(), 0.into()));
		}

		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(0, MIN_SQRT_PRICE).unwrap(),
			};

			let amount: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.collect_and_mint_limit_order(
					&LiquidityProvider::from([0; 32]),
					!order,
					0,
					amount
				)),
				(Default::default(), limit_orders::PositionInfo::new(amount))
			);

			assert_eq!(pool_state.swap(order, 0.into(), None), (0.into(), 0.into()));
			assert_eq!(pool_state.swap(order, Amount::MAX, None), (amount, Amount::MAX - amount));
		}

		{
			let initial_sqrt_price = match order.to_sold_pair() {
				Pairs::Base => MAX_SQRT_PRICE,
				Pairs::Quote => MIN_SQRT_PRICE,
			};
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
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

			assert_eq!(pool_state.swap(order, 0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap(order, Amount::MAX, None),
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
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
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
				assert_ok!(pool_state.collect_and_mint_limit_order(
					&LiquidityProvider::from([0; 32]),
					!order,
					0,
					limit_order_liquidity
				)),
				(Default::default(), limit_orders::PositionInfo::new(limit_order_liquidity))
			);

			assert_eq!(pool_state.swap(order, 0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap(order, Amount::MAX, None),
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
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
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
				assert_ok!(pool_state.collect_and_mint_limit_order(
					&LiquidityProvider::from([0; 32]),
					!order,
					0,
					limit_order_liquidity
				)),
				(Default::default(), limit_orders::PositionInfo::new(limit_order_liquidity))
			);

			assert_eq!(pool_state.swap(order, 0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap(order, Amount::MAX, None),
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

	let mut pool = PoolState::new(500, Price::at_tick_zero()).unwrap();
	pool.collect_and_mint_limit_order(&LP, Side::Buy, 0, 1_000_000.into()).unwrap();

	pool.collect_and_mint_range_order(
		&LP,
		TICK_RANGE,
		Size::Liquidity { liquidity: 1_000_000_000_000 },
		Result::<_, Infallible>::Ok,
	)
	.unwrap();

	pool.collect_and_mint_limit_order(&LP, Side::Buy, 10, 1_000_000.into()).unwrap();

	assert_eq!(pool.limit_order(&LP, Side::Buy, 0).unwrap().0.sold_amount, 0.into());
	assert_eq!(pool.limit_order(&LP, Side::Buy, 10).unwrap().0.sold_amount, 0.into());
	assert_eq!(pool.range_order(&LP, TICK_RANGE).unwrap().0.fees.base, 0.into());

	pool.swap(Side::Sell, 3_000_000.into(), None);

	// Check that all three orders have been used in the swap:
	assert!(pool.limit_order(&LP, Side::Buy, 0).unwrap().0.sold_amount > 0.into());
	assert!(pool.limit_order(&LP, Side::Buy, 10).unwrap().0.sold_amount > 0.into());
	assert!(pool.range_order(&LP, TICK_RANGE).unwrap().0.fees.base > 0.into());
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
			let sqrt_price = SqrtPriceQ64F96::from(price);
			let adjusted_sqrt_price =
				sqrt_price_adjusted_by_pool_fee::<SD>(sqrt_price, fee_hundredth_pips);
			let adjusted_price = Price::from(adjusted_sqrt_price);

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
