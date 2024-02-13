#![cfg(test)]

use cf_utilities::assert_ok;
use core::convert::Infallible;

use crate::{
	common::{sqrt_price_to_price, Price, MAX_SQRT_PRICE, MIN_SQRT_PRICE, PRICE_FRACTIONAL_BITS},
	range_orders::Liquidity,
};

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

#[test]
fn test_sqrt_price_to_price() {
	assert_eq!(
		sqrt_price_to_price(SqrtPriceQ64F96::from(1) << 96),
		Price::from(1) << PRICE_FRACTIONAL_BITS
	);
	assert!(sqrt_price_to_price(MIN_SQRT_PRICE) < sqrt_price_to_price(MAX_SQRT_PRICE));
}
