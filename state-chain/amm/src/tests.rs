#![cfg(test)]

use cf_utilities::assert_ok;
use core::convert::Infallible;

use crate::{
	common::{MAX_SQRT_PRICE, MIN_SQRT_PRICE},
	range_orders::Liquidity,
};

use super::*;

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

#[test]
fn test_basic_swaps() {
	fn inner<
		SD: common::SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection,
	>() {
		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(0, MIN_SQRT_PRICE).unwrap(),
			};

			assert_eq!(pool_state.swap::<SD>(0.into(), None), (0.into(), 0.into()));
			assert_eq!(pool_state.swap::<SD>(Amount::MAX, None), (0.into(), Amount::MAX));
			assert_eq!(pool_state.swap::<SD>(0.into(), Some(0.into())), (0.into(), 0.into()));
		}

		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(0, MIN_SQRT_PRICE).unwrap(),
			};

			let amount: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.limit_orders.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					amount
				)),
				(Default::default(), limit_orders::PositionInfo::new(amount))
			);

			assert_eq!(pool_state.swap::<SD>(0.into(), None), (0.into(), 0.into()));
			assert_eq!(pool_state.swap::<SD>(Amount::MAX, None), (amount, Amount::MAX - amount));
		}

		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(
					0,
					match SD::INPUT_SIDE {
						common::Side::Zero => MAX_SQRT_PRICE - 1,
						common::Side::One => MIN_SQRT_PRICE,
					},
				)
				.unwrap(),
			};

			let liquidity: range_orders::Liquidity = 10000;

			let (minted_amounts, minted_liquidity, collected_fees, position_info) =
				assert_ok!(pool_state.range_orders.collect_and_mint(
					&LiquidityProvider::from([0; 32]),
					-100,
					100,
					range_orders::Size::Liquidity { liquidity },
					Result::<_, Infallible>::Ok
				));
			assert_eq!(minted_liquidity, liquidity);
			assert_eq!(collected_fees, Default::default());
			assert_eq!(position_info, range_orders::PositionInfo::new(liquidity));

			assert_eq!(pool_state.swap::<SD>(0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap::<SD>(Amount::MAX, None),
				(
					minted_amounts[!SD::INPUT_SIDE] - 1, /* -1 is due to rounding down */
					Amount::MAX - minted_amounts[!SD::INPUT_SIDE]
				)
			);
		}

		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(
					0,
					match SD::INPUT_SIDE {
						common::Side::Zero => MAX_SQRT_PRICE - 1,
						common::Side::One => MIN_SQRT_PRICE,
					},
				)
				.unwrap(),
			};

			let range_order_liquidity: Liquidity = 10000;

			let (range_order_minted_amounts, minted_liquidity, collected_fees, position_info) =
				assert_ok!(pool_state.range_orders.collect_and_mint(
					&LiquidityProvider::from([0; 32]),
					-100,
					100,
					range_orders::Size::Liquidity { liquidity: range_order_liquidity },
					Result::<_, Infallible>::Ok
				));
			assert_eq!(minted_liquidity, range_order_liquidity);
			assert_eq!(collected_fees, Default::default());
			assert_eq!(position_info, range_orders::PositionInfo::new(range_order_liquidity));

			let limit_order_liquidity: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.limit_orders.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					limit_order_liquidity
				)),
				(Default::default(), limit_orders::PositionInfo::new(limit_order_liquidity))
			);

			assert_eq!(pool_state.swap::<SD>(0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap::<SD>(Amount::MAX, None),
				(
					limit_order_liquidity + range_order_minted_amounts[!SD::INPUT_SIDE] - 1, /* -1 is due to rounding down */
					Amount::MAX -
						(limit_order_liquidity + range_order_minted_amounts[!SD::INPUT_SIDE]) -
						1  /* -1 is due to rounding down */
				)
			);
		}

		{
			let mut pool_state = PoolState {
				limit_orders: limit_orders::PoolState::new(0).unwrap(),
				range_orders: range_orders::PoolState::new(
					0,
					match SD::INPUT_SIDE {
						common::Side::Zero => MAX_SQRT_PRICE - 1,
						common::Side::One => MIN_SQRT_PRICE,
					},
				)
				.unwrap(),
			};

			let mut mint_range_order = |lower_tick, upper_tick| {
				let liquidity = 100000;
				let (range_order_minted_amounts, minted_liquidity, collected_fees, position_info) =
					assert_ok!(pool_state.range_orders.collect_and_mint(
						&LiquidityProvider::from([0; 32]),
						lower_tick,
						upper_tick,
						range_orders::Size::Liquidity { liquidity },
						Result::<_, Infallible>::Ok
					));
				assert_eq!(minted_liquidity, liquidity);
				assert_eq!(collected_fees, Default::default());
				assert_eq!(position_info, range_orders::PositionInfo::new(100000));

				range_order_minted_amounts
			};
			let range_order_minted_amounts =
				mint_range_order(-100, -10) + mint_range_order(10, 100);

			let limit_order_liquidity: Amount = 10000.into();
			assert_eq!(
				assert_ok!(pool_state.limit_orders.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					limit_order_liquidity
				)),
				(Default::default(), limit_orders::PositionInfo::new(limit_order_liquidity))
			);

			assert_eq!(pool_state.swap::<SD>(0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap::<SD>(Amount::MAX, None),
				(
					limit_order_liquidity + range_order_minted_amounts[!SD::INPUT_SIDE] - 2, /* -2 is due to rounding down */
					Amount::MAX -
						(limit_order_liquidity + range_order_minted_amounts[!SD::INPUT_SIDE])
				)
			);
		}
	}

	inner::<common::ZeroToOne>();
	inner::<common::OneToZero>();
}
