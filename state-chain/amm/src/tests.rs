use cf_utilities::assert_ok;
use sp_core::crypto::Infallible;

use crate::common::{MAX_SQRT_PRICE, MIN_SQRT_PRICE};

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

			let liquidity: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.limit_orders.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					liquidity
				)),
				Default::default()
			);

			assert_eq!(pool_state.swap::<SD>(0.into(), None), (0.into(), 0.into()));
			assert_eq!(
				pool_state.swap::<SD>(Amount::MAX, None),
				(liquidity, Amount::MAX - liquidity)
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

			let liquidity: range_orders::Liquidity = 10000;

			let (minted_amounts, collected_fees) =
				assert_ok!(pool_state.range_orders.collect_and_mint(
					&LiquidityProvider::from([0; 32]),
					-100,
					100,
					liquidity,
					Result::<_, Infallible>::Ok
				));
			assert_eq!(collected_fees, Default::default());

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

			let (range_order_minted_amounts, collected_fees) =
				assert_ok!(pool_state.range_orders.collect_and_mint(
					&LiquidityProvider::from([0; 32]),
					-100,
					100,
					10000,
					Result::<_, Infallible>::Ok
				));
			assert_eq!(collected_fees, Default::default());

			let limit_order_liquidity: Amount = 10000.into();

			assert_eq!(
				assert_ok!(pool_state.limit_orders.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					limit_order_liquidity
				)),
				Default::default()
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
				let (range_order_minted_amounts, collected_fees) =
					assert_ok!(pool_state.range_orders.collect_and_mint(
						&LiquidityProvider::from([0; 32]),
						lower_tick,
						upper_tick,
						100000,
						Result::<_, Infallible>::Ok
					));
				assert_eq!(collected_fees, Default::default());

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
				Default::default()
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
