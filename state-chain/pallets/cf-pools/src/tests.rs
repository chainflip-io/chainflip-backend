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

use crate::{self as pallet_cf_pools, mock::*, *};
use cf_amm::{common::Side, math::Tick};
use cf_primitives::{chains::assets::any::Asset, AssetAmount};
use cf_test_utilities::{
	assert_events_eq, assert_events_match, assert_matching_event_count, last_event,
};
use cf_traits::{
	mocks::balance_api::MockBalance, BalanceApi, PoolApi, PoolOrdersManager, SwappingApi,
};
use frame_support::{assert_noop, assert_ok};
use sp_core::bounded_vec;
use sp_runtime::BoundedVec;

#[test]
fn can_create_new_trading_pool() {
	new_test_ext().execute_with(|| {
		let unstable_asset = Asset::Eth;
		let default_price = Price::at_tick_zero();

		// While the pool does not exist, no info can be obtained.
		assert!(Pools::<Test>::get(AssetPair::new(unstable_asset, STABLE_ASSET).unwrap()).is_none());

		// Fee must be appropriate
		assert_noop!(
			LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				unstable_asset,
				STABLE_ASSET,
				1_000_000u32,
				default_price,
			),
			Error::<Test>::InvalidFeeAmount,
		);

		// Make sure only governance can create a new pool.
		assert_noop!(
			LiquidityPools::new_pool(
				RuntimeOrigin::signed(ALICE),
				unstable_asset,
				STABLE_ASSET,
				500_000u32,
				default_price,
			),
			sp_runtime::traits::BadOrigin
		);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			STABLE_ASSET,
			500_000u32,
			default_price,
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(Event::<Test>::NewPoolCreated {
			base_asset: unstable_asset,
			quote_asset: STABLE_ASSET,
			fee_hundredth_pips: 500_000u32,
			initial_price: default_price,
		}));

		// Cannot create duplicate pool
		assert_noop!(
			LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				unstable_asset,
				STABLE_ASSET,
				0u32,
				default_price
			),
			Error::<Test>::PoolAlreadyExists
		);
	});
}

#[test]
fn test_mint_range_order_with_asset_amounts() {
	new_test_ext().execute_with(|| {
		const POSITION: core::ops::Range<Tick> = -100_000..100_000;
		const FLIP: Asset = Asset::Flip;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			FLIP,
			STABLE_ASSET,
			Default::default(),
			Price::at_tick_zero(),
		));

		MockBalance::credit_account(&ALICE, FLIP, 1_000_000);
		MockBalance::credit_account(&ALICE, STABLE_ASSET, 1_000_000);

		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			0,
			Some(POSITION),
			RangeOrderSize::AssetAmounts {
				maximum: AssetAmounts { base: 1_000_000, quote: 1_000_000 },
				minimum: AssetAmounts { base: 900_000, quote: 900_000 },
			}
		));
		assert_events_match!(
			Test,
			RuntimeEvent::LiquidityPools(
				Event::RangeOrderUpdated {
					..
				},
			) => ()
		);
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			0,
			Some(POSITION),
			RangeOrderSize::Liquidity { liquidity: 0 }
		));
	});
}

#[test]
fn pallet_limit_order_is_in_sync_with_pool() {
	new_test_ext().execute_with(|| {
		let tick = 100;
		let asset_pair = AssetPair::new(Asset::Eth, STABLE_ASSET).unwrap();

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Price::at_tick_zero(),
		));

		MockBalance::credit_account(&ALICE, Asset::Eth, 100);
		MockBalance::credit_account(&BOB, Asset::Eth, 100_000);
		MockBalance::credit_account(&BOB, STABLE_ASSET, 10_000);

		// Setup liquidity for the pool with 2 LPer
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(0),
			100,
			None,
			None,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(tick),
			100_000,
			None,
			None,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Side::Buy,
			1,
			Some(tick),
			10_000,
			None,
			None,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BTreeSet::from([ALICE]), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: ALICE,
						id: 0.into(),
						tick: 0,
						sell_amount: 100.into(),
						fees_earned: 0.into(),
						original_sell_amount: 100.into()
					}],
					bids: vec![]
				},
				range_orders: vec![]
			})
		);

		let pallet_limit_orders = Pools::<Test>::get(asset_pair).unwrap().limit_orders_cache;
		assert_eq!(pallet_limit_orders.base[&ALICE][&0], 0);
		assert_eq!(pallet_limit_orders.base[&BOB][&0], tick);
		assert_eq!(pallet_limit_orders.quote[&BOB][&1], tick);

		// Do some swaps to execute limit orders
		LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, 101_000).unwrap();
		LiquidityPools::swap_single_leg(Asset::Eth, STABLE_ASSET, 9_900).unwrap();

		assert_ok!(LiquidityPools::sweep(&ALICE));
		assert_ok!(LiquidityPools::sweep(&BOB));

		// 100 swapped. The position is fully consumed.
		assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 100);
		assert_eq!(MockBalance::get_balance(&ALICE, Asset::Eth), 0);

		let pallet_limit_orders = Pools::<Test>::get(asset_pair).unwrap().limit_orders_cache;
		assert_eq!(pallet_limit_orders.base.get(&ALICE), None);
		assert_eq!(pallet_limit_orders.base.get(&BOB).unwrap().get(&0), Some(&tick));

		// Expect two events: one event for creation, one for sweeping.
		assert_matching_event_count!(
			Test,
			RuntimeEvent::LiquidityPools(Event::LimitOrderUpdated {
				lp: ALICE,
				side: Side::Sell,
				..
			}) => 2
		);

		assert_matching_event_count!(
			Test,
			RuntimeEvent::LiquidityPools(Event::LimitOrderUpdated { lp: BOB, side: Side::Buy, .. }) => 2
		);

		assert_matching_event_count!(
			Test,
			RuntimeEvent::LiquidityPools(Event::LimitOrderUpdated {
				lp: BOB,
				side: Side::Sell,
				..
			}) => 2
		);
	});
}

#[test]
fn update_pool_liquidity_fee_collects_fees_for_range_order() {
	new_test_ext().execute_with(|| {
		let range = -100..100;
		let old_fee = 400_000u32;
		let new_fee = 100_000u32;
		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			old_fee,
			Price::at_tick_zero(),
		));
		assert_eq!(
			LiquidityPools::pool_info(Asset::Eth, STABLE_ASSET),
			Ok(PoolInfo {
				range_order_fee_hundredth_pips: old_fee,
				range_order_total_fees_earned: Default::default(),
				range_total_swap_inputs: Default::default(),
				limit_total_swap_inputs: Default::default(),
			})
		);

		MockBalance::credit_account(&ALICE, Asset::Eth, 4988);
		MockBalance::credit_account(&ALICE, STABLE_ASSET, 4988);
		MockBalance::credit_account(&BOB, Asset::Eth, 4988);
		MockBalance::credit_account(&BOB, STABLE_ASSET, 4988);

		// Setup liquidity for the pool with 2 LPer with range orders
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(range.clone()),
			RangeOrderSize::Liquidity { liquidity: 1_000_000 },
		));

		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(range.clone()),
			RangeOrderSize::Liquidity { liquidity: 1_000_000 },
		));

		// Do some swaps to collect fees.
		LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, 5_000).unwrap();
		LiquidityPools::swap_single_leg(Asset::Eth, STABLE_ASSET, 5_000).unwrap();

		// Updates the fees to the new value. No fee is collected for range orders.
		assert_ok!(LiquidityPools::set_pool_fees(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			new_fee
		));

		assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 0);
		assert_eq!(MockBalance::get_balance(&ALICE, Asset::Eth), 0);
		assert_eq!(MockBalance::get_balance(&BOB, STABLE_ASSET), 0);
		assert_eq!(MockBalance::get_balance(&BOB, Asset::Eth), 0);

		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BTreeSet::from([ALICE]), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap { asks: vec![], bids: vec![] },
				range_orders: vec![RangeOrder {
					lp: ALICE,
					id: 0.into(),
					range: range.clone(),
					liquidity: 1_000_000,
					fees_earned: PoolPairsMap { base: 999.into(), quote: 999.into() }
				}]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BTreeSet::from([BOB]), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap { asks: vec![], bids: vec![] },
				range_orders: vec![RangeOrder {
					lp: BOB,
					id: 0.into(),
					range: range.clone(),
					liquidity: 1_000_000,
					fees_earned: PoolPairsMap { base: 999.into(), quote: 999.into() }
				}]
			})
		);

		// Cash out the liquidity will payout earned fee
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(range.clone()),
			RangeOrderSize::Liquidity { liquidity: 0 },
		));
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(range.clone()),
			RangeOrderSize::Liquidity { liquidity: 0 },
		));

		// Earned liquidity pool fees are paid out.
		// Total of ~ 4_000 fee were paid, evenly split between Alice and Bob.
		assert_eq!(MockBalance::get_balance(&ALICE, Asset::Eth), 5988);
		assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 5984);
		assert_eq!(MockBalance::get_balance(&BOB, Asset::Eth), 5988);
		assert_eq!(MockBalance::get_balance(&BOB, STABLE_ASSET), 5984);
	});
}

#[test]
fn can_execute_scheduled_limit_order_updates() {
	fn test_scheduled_limit_order_update(
		update: impl Fn(OrderId, u64, AssetAmount) -> DispatchResult,
	) {
		const DISPATCH_AT: u64 = 6;
		const ORDER_ID: u64 = 0;
		const AMOUNT: AssetAmount = 55;

		new_test_ext()
			.execute_with(|| {
				assert_ok!(LiquidityPools::new_pool(
					RuntimeOrigin::root(),
					Asset::Flip,
					STABLE_ASSET,
					400_000u32,
					Price::at_tick_zero(),
				));

				MockBalance::credit_account(&ALICE, STABLE_ASSET, AMOUNT);
				assert_ok!(update(ORDER_ID, DISPATCH_AT, AMOUNT));
				assert_eq!(
					last_event::<Test>(),
					RuntimeEvent::LiquidityPools(Event::LimitOrderSetOrUpdateScheduled {
						lp: ALICE,
						order_id: ORDER_ID,
						dispatch_at: DISPATCH_AT,
					})
				);
				assert!(!ScheduledLimitOrderUpdates::<Test>::get(DISPATCH_AT).is_empty());
			})
			.then_process_blocks_until_block(DISPATCH_AT)
			.then_execute_with(|_| {
				assert!(ScheduledLimitOrderUpdates::<Test>::get(DISPATCH_AT).is_empty());
				assert_eq!(
					last_event::<Test>(),
					RuntimeEvent::LiquidityPools(Event::ScheduledLimitOrderUpdateDispatchSuccess {
						lp: ALICE,
						order_id: ORDER_ID,
					})
				);
			});
	}

	test_scheduled_limit_order_update(
		|order_id: OrderId, dispatch_at: u64, amount: AssetAmount| {
			LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(ALICE),
				Asset::Flip,
				STABLE_ASSET,
				Side::Buy,
				order_id,
				Some(100),
				amount,
				Some(dispatch_at),
				None,
			)
		},
	);

	test_scheduled_limit_order_update(
		|order_id: OrderId, dispatch_at: u64, amount: AssetAmount| {
			LiquidityPools::update_limit_order(
				RuntimeOrigin::signed(ALICE),
				Asset::Flip,
				STABLE_ASSET,
				Side::Buy,
				order_id,
				Some(100),
				IncreaseOrDecrease::Increase(amount),
				Some(dispatch_at),
			)
		},
	);
}

#[test]
fn test_dispatch_at_validation() {
	const CURRENT_BLOCK: u64 = 10;
	new_test_ext().then_execute_at_block(10u32, |_| {
		assert_noop!(
			LiquidityPools::update_limit_order(
				RuntimeOrigin::signed(ALICE),
				Asset::Flip,
				STABLE_ASSET,
				Side::Buy,
				0,
				Some(0),
				IncreaseOrDecrease::Decrease(55),
				// 1 block in the past
				Some(CURRENT_BLOCK - 1)
			),
			Error::<Test>::InvalidDispatchAt
		);

		assert_noop!(
			LiquidityPools::update_limit_order(
				RuntimeOrigin::signed(ALICE),
				Asset::Flip,
				STABLE_ASSET,
				Side::Buy,
				0,
				Some(0),
				IncreaseOrDecrease::Decrease(55),
				// Too far in the future
				Some(CURRENT_BLOCK + (SCHEDULE_UPDATE_LIMIT_BLOCKS as u64) + 1)
			),
			Error::<Test>::InvalidDispatchAt
		);

		assert_ok!(LiquidityPools::update_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Flip,
			STABLE_ASSET,
			Side::Buy,
			0,
			Some(0),
			IncreaseOrDecrease::Decrease(55),
			// Valid dispatch at
			Some(CURRENT_BLOCK + (SCHEDULE_UPDATE_LIMIT_BLOCKS as u64))
		));
	});
}

#[test]
fn can_get_all_pool_orders() {
	new_test_ext().execute_with(|| {
		let range_1 = -100..100;
		let range_2 = -234..234;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			Default::default(),
			Price::at_tick_zero(),
		));

		MockBalance::credit_account(&ALICE, STABLE_ASSET, 10_000_000);
		MockBalance::credit_account(&ALICE, Asset::Eth, 10_000_000);
		MockBalance::credit_account(&BOB, STABLE_ASSET, 10_000_000);
		MockBalance::credit_account(&BOB, Asset::Eth, 10_000_000);

		// Setup liquidity for the pool with 2 LPer, each has limit and range orders.
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(range_1.clone()),
			RangeOrderSize::Liquidity { liquidity: 100_000 },
		));
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			1,
			Some(range_2.clone()),
			RangeOrderSize::Liquidity { liquidity: 200_000 },
		));
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			2,
			Some(range_1.clone()),
			RangeOrderSize::Liquidity { liquidity: 300_000 },
		));
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			3,
			Some(range_2.clone()),
			RangeOrderSize::Liquidity { liquidity: 400_000 },
		));

		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			4,
			Some(100),
			500_000,
			None,
			None,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			5,
			Some(1000),
			600_000,
			None,
			None,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			6,
			Some(100),
			700_000,
			None,
			None,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Buy,
			7,
			Some(1000),
			800_000,
			None,
			None,
		));

		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BTreeSet::new(), false),
			Ok(PoolOrders::<Test> {
				limit_orders: AskBidMap {
					asks: vec![
						LimitOrder {
							lp: ALICE,
							id: 4.into(),
							tick: 100,
							sell_amount: 500_000.into(),
							fees_earned: 0.into(),
							original_sell_amount: 500_000.into(),
						},
						LimitOrder {
							lp: ALICE,
							id: 5.into(),
							tick: 1000,
							sell_amount: 600_000.into(),
							fees_earned: 0.into(),
							original_sell_amount: 600_000.into(),
						},
						LimitOrder {
							lp: ALICE,
							id: 6.into(),
							tick: 100,
							sell_amount: 700_000.into(),
							fees_earned: 0.into(),
							original_sell_amount: 700_000.into(),
						}
					],
					bids: vec![LimitOrder {
						lp: ALICE,
						id: 7.into(),
						tick: 1000,
						sell_amount: 800_000.into(),
						fees_earned: 0.into(),
						original_sell_amount: 800_000.into(),
					}]
				},
				range_orders: vec![
					RangeOrder {
						lp: ALICE,
						id: 0.into(),
						range: -100..100,
						liquidity: 100_000u128,
						fees_earned: Default::default(),
					},
					RangeOrder {
						lp: ALICE,
						id: 1.into(),
						range: -234..234,
						liquidity: 200_000u128,
						fees_earned: Default::default(),
					},
					RangeOrder {
						lp: BOB,
						id: 2.into(),
						range: -100..100,
						liquidity: 300_000u128,
						fees_earned: Default::default(),
					},
					RangeOrder {
						lp: BOB,
						id: 3.into(),
						range: -234..234,
						liquidity: 400_000u128,
						fees_earned: Default::default(),
					}
				]
			})
		);
	});
}

#[test]
fn range_order_fees_are_recorded() {
	new_test_ext().execute_with(|| {
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			100,
			Price::at_tick_zero(),
		));

		MockBalance::credit_account(&ALICE, STABLE_ASSET, 1_000_000_000_000_000_000);
		MockBalance::credit_account(&ALICE, Asset::Eth, 1_000_000_000_000_000_000);

		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(-100..100),
			RangeOrderSize::Liquidity { liquidity: 1_000_000_000_000_000_000 },
		));

		assert!(
			LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, 1_000_000_000).unwrap() > 0
		);
		LiquidityPools::sweep(&ALICE).unwrap();

		assert!(
			HistoricalEarnedFees::<Test>::get(ALICE, Asset::Usdc) > 0,
			"Alice's fees should be recorded but are:{:?}",
			HistoricalEarnedFees::<Test>::iter_prefix(ALICE).collect::<Vec<_>>(),
		);
	});
}

#[test]
fn test_maximum_slippage_limits() {
	use cf_utilities::{assert_err, assert_ok};

	new_test_ext().execute_with(|| {
		const BASE_ASSET: Asset = Asset::Eth;
		const OTHER_ASSET: Asset = Asset::Btc;

		let asset_pair = AssetPair::new(BASE_ASSET, STABLE_ASSET).unwrap();

		// Ensure limits are configured per pool: this limit should be ignored during testing.
		assert_ok!(LiquidityPools::set_maximum_price_impact(
			RuntimeOrigin::root(),
			bounded_vec![(OTHER_ASSET, Some(1))],
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(Event::PriceImpactLimitSet {
			asset_pair: AssetPair::new(OTHER_ASSET, STABLE_ASSET).unwrap(),
			limit: Some(1),
		}));

		MockBalance::credit_account(&ALICE, STABLE_ASSET, 10_000_000);
		MockBalance::credit_account(&ALICE, BASE_ASSET, 10_000_000);

		let test_swaps = |size_limit_when_slippage_limit_is_hit| {
			for (size, expected_output) in [
				(0, 0),
				(1, 0),
				(100, 99),
				(200, 199),
				(250, 249),
				(300, 299),
				(400, 398),
				(500, 497),
				(1500, 1477),
				(2500, 2439),
				(3500, 3381),
				(4500, 4306),
				(5500, 5213),
				(6500, 6103),
				(7500, 6976),
				(8500, 7834),
				(9500, 8675),
				(10500, 9502),
				(11500, 10313),
				(12500, 11111),
				(13500, 11894),
				(14500, 12663),
				(15500, 13419),
			] {
				pallet_cf_pools::Pools::<Test>::remove(asset_pair);
				assert_ok!(LiquidityPools::new_pool(
					RuntimeOrigin::root(),
					BASE_ASSET,
					STABLE_ASSET,
					Default::default(),
					Price::at_tick_zero(),
				));
				assert_ok!(LiquidityPools::set_range_order(
					RuntimeOrigin::signed(ALICE),
					BASE_ASSET,
					STABLE_ASSET,
					0,
					Some(-10000..10000),
					RangeOrderSize::Liquidity { liquidity: 100_000 },
				));
				let result = LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, size);
				if size < size_limit_when_slippage_limit_is_hit {
					assert_eq!(expected_output, assert_ok!(result));
				} else {
					assert_err!(result);
				}
			}
		};

		test_swaps(u128::MAX);

		assert_ok!(LiquidityPools::set_maximum_price_impact(
			RuntimeOrigin::root(),
			bounded_vec![(BASE_ASSET, Some(954))]
		));

		test_swaps(10500);

		assert_ok!(LiquidityPools::set_maximum_price_impact(
			RuntimeOrigin::root(),
			bounded_vec![(BASE_ASSET, None)]
		));

		test_swaps(u128::MAX);

		assert_ok!(LiquidityPools::set_maximum_price_impact(
			RuntimeOrigin::root(),
			bounded_vec![(BASE_ASSET, Some(10))]
		));

		test_swaps(300);

		assert_ok!(LiquidityPools::set_maximum_price_impact(
			RuntimeOrigin::root(),
			bounded_vec![(BASE_ASSET, Some(300))]
		));

		test_swaps(3500);
	});
}

#[test]
fn can_accept_additional_limit_orders() {
	new_test_ext().execute_with(|| {
		let from = Asset::Flip;
		let to = Asset::Usdt;
		let default_price = Price::at_tick_zero();

		for asset in [from, to] {
			// While the pool does not exist, no info can be obtained.
			assert!(Pools::<Test>::get(AssetPair::new(asset, STABLE_ASSET).unwrap()).is_none());

			// Create a new pool.
			assert_ok!(LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				asset,
				STABLE_ASSET,
				0u32,
				default_price,
			));
		}

		const ONE_FLIP: u128 = 10u128.pow(18);

		assert!(LiquidityPools::swap_single_leg(from, STABLE_ASSET, ONE_FLIP,).is_err());

		assert!(LiquidityPools::try_add_limit_order(
			&0,
			from,
			STABLE_ASSET,
			Side::Buy,
			0,
			-196236,
			ONE_FLIP.into()
		)
		.is_ok());

		let first_leg = LiquidityPools::swap_single_leg(from, STABLE_ASSET, ONE_FLIP).unwrap();
		assert_eq!(first_leg, 3006110201);

		const ONE_USDC: u128 = 10u128.pow(6);

		assert!(LiquidityPools::try_add_limit_order(
			&0,
			from,
			Asset::Usdc,
			Side::Buy,
			0,
			-196236,
			ONE_FLIP.into()
		)
		.is_ok());
		assert!(LiquidityPools::swap_single_leg(STABLE_ASSET, to, first_leg).is_err());

		assert!(LiquidityPools::try_add_limit_order(
			&0,
			from,
			Asset::Usdc,
			Side::Buy,
			0,
			-196236,
			ONE_FLIP.into()
		)
		.is_ok());
		assert!(LiquidityPools::try_add_limit_order(
			&0,
			to,
			Asset::Usdc,
			Side::Sell,
			0,
			0,
			(3500 * ONE_USDC).into()
		)
		.is_ok());

		assert_eq!(
			LiquidityPools::swap_single_leg(STABLE_ASSET, to, first_leg).unwrap(),
			3006110200
		);
	});
}

#[test]
fn test_cancel_orders_batch() {
	new_test_ext().execute_with(|| {
		const POSITION: core::ops::Range<Tick> = -1_000..1_000;
		const FLIP: Asset = Asset::Flip;
		const TICK: Tick = 5;
		const POOL_FEE_BPS: u32 = 5;

		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			FLIP,
			STABLE_ASSET,
			POOL_FEE_BPS * 100,
			Price::at_tick_zero(),
		));

		MockBalance::credit_account(&ALICE, STABLE_ASSET, 1_000_000);
		MockBalance::credit_account(&ALICE, FLIP, 2_000_000);

		// Set up range and limit orders such that both are executed
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			0,
			Some(POSITION),
			RangeOrderSize::AssetAmounts {
				maximum: AssetAmounts { base: 1_000_000, quote: 1_000_000 },
				minimum: AssetAmounts { base: 900_000, quote: 900_000 },
			}
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(TICK),
			5_000,
			None,
			None,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			Side::Sell,
			1,
			Some(TICK + POOL_FEE_BPS as Tick),
			15_000,
			None,
			None,
		));

		assert_eq!(
			LiquidityPools::open_order_count(
				&ALICE,
				&PoolPairsMap { base: FLIP, quote: STABLE_ASSET }
			)
			.unwrap(),
			3
		);

		// Do a swap and check that the fee has not been collected yet
		assert!(LiquidityPools::swap_single_leg(STABLE_ASSET, FLIP, 15_000).is_ok());
		assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 0);
		assert_eq!(HistoricalEarnedFees::<Test>::get(ALICE, STABLE_ASSET), 0);

		assert_ok!(LiquidityPools::cancel_orders_batch(
			RuntimeOrigin::signed(ALICE),
			vec![
				CloseOrder::Limit {
					base_asset: FLIP,
					quote_asset: STABLE_ASSET,
					side: Side::Sell,
					id: 0,
				},
				CloseOrder::Limit {
					base_asset: FLIP,
					quote_asset: STABLE_ASSET,
					side: Side::Sell,
					id: 1,
				},
				CloseOrder::Range { base_asset: FLIP, quote_asset: STABLE_ASSET, id: 0 },
			]
			.try_into()
			.unwrap()
		));
		assert_eq!(
			LiquidityPools::open_order_count(
				&ALICE,
				&PoolPairsMap { base: FLIP, quote: STABLE_ASSET }
			)
			.unwrap(),
			0
		);
		// Canceling the orders should have also swept the orders
		assert!(HistoricalEarnedFees::<Test>::get(ALICE, STABLE_ASSET) > 0);
	});
}

#[test]
fn only_governance_can_set_pool_fee() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityPools::set_pool_fees(
				RuntimeOrigin::signed(ALICE),
				Asset::Eth,
				STABLE_ASSET,
				0
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn only_governance_can_set_maximum_price_impact() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityPools::set_maximum_price_impact(
				RuntimeOrigin::signed(ALICE),
				BoundedVec::try_from(vec![(Asset::Eth, None)]).unwrap()
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn handle_zero_liquidity_changes_set_range_order() {
	new_test_ext().execute_with(|| {
		const POSITION: core::ops::Range<Tick> = -887272..887272;
		const FLIP: Asset = Asset::Flip;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			FLIP,
			STABLE_ASSET,
			Default::default(),
			Price::at_tick_zero(),
		));

		assert_noop!(
			LiquidityPools::set_range_order(
				RuntimeOrigin::signed(ALICE),
				FLIP,
				STABLE_ASSET,
				0,
				Some(POSITION),
				RangeOrderSize::AssetAmounts {
					maximum: AssetAmounts { base: 1, quote: 0 },
					minimum: AssetAmounts { base: 0, quote: 0 },
				}
			),
			crate::Error::<Test>::InvalidSize
		);
	});
}

#[test]
fn auto_sweeping() {
	const ASSET: Asset = Asset::Usdt;

	let get_balance =
		|lp| (MockBalance::get_balance(lp, ASSET), MockBalance::get_balance(lp, STABLE_ASSET));

	new_test_ext()
		.execute_with(|| {
			assert_ok!(LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				ASSET,
				STABLE_ASSET,
				0,
				Price::at_tick_zero(),
			));

			for (lp, amount) in [(ALICE, 20_000), (BOB, 10_000)] {
				MockBalance::credit_account(&lp, ASSET, amount);
				assert_ok!(LiquidityPools::set_limit_order(
					RuntimeOrigin::signed(lp),
					ASSET,
					STABLE_ASSET,
					Side::Sell,
					1,
					Some(100),
					amount,
					None,
					None,
				));
			}

			// Setting different thresholds for different assets to improve coverage:
			LimitOrderAutoSweepingThresholds::<Test>::mutate(|thresholds| {
				thresholds.try_insert(ASSET, 5_000).unwrap();
				thresholds.try_insert(STABLE_ASSET, 10_000).unwrap();
			});

			assert_eq!(get_balance(&ALICE), (0, 0));
			assert_eq!(get_balance(&BOB), (0, 0));

			assert!(LiquidityPools::swap_single_leg(STABLE_ASSET, ASSET, 20_000).is_ok());
		})
		.then_execute_at_next_block(|_| {
			// Alice's funds should have been swept, but not yet Bob's:
			assert_eq!(get_balance(&ALICE), (0, 13_332));
			assert_eq!(get_balance(&BOB), (0, 0));

			// Another swap should result in Bob's orders being swept too:
			assert!(LiquidityPools::swap_single_leg(STABLE_ASSET, ASSET, 10_100).is_ok());
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(get_balance(&ALICE), (0, 13_332));
			assert_eq!(get_balance(&BOB), (0, 10_032));

			// Check that auto-sweeping works in the other direction too
			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(ALICE),
				ASSET,
				STABLE_ASSET,
				Side::Buy,
				1,
				Some(0),
				5_000,
				None,
				None,
			));

			// Note: increase due to implicit sweeping in `set_limit_order`
			assert_eq!(get_balance(&ALICE), (0, 15_063));

			// The amount in this swap is not sufficient to trigger auto sweeping:
			assert!(LiquidityPools::swap_single_leg(ASSET, STABLE_ASSET, 3_000).is_ok());
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(get_balance(&ALICE), (0, 15_063));

			// This swap should take us over the threshold for ASSET:
			assert!(LiquidityPools::swap_single_leg(ASSET, STABLE_ASSET, 2_000).is_ok());
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(get_balance(&ALICE), (5000, 15_063));
		});
}

#[test]
fn cancel_all_limit_orders_for_account() {
	const ASSET_1: Asset = Asset::Usdt;
	const ASSET_2: Asset = Asset::Btc;
	const ORDER_ID: u64 = 1;

	new_test_ext().execute_with(|| {
		for asset in [ASSET_1, ASSET_2] {
			assert_ok!(LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				asset,
				STABLE_ASSET,
				0,
				Price::at_tick_zero(),
			));
		}

		for (lp, base_asset, side) in [
			(ALICE, ASSET_1, Side::Sell),
			(ALICE, ASSET_1, Side::Buy),
			(ALICE, ASSET_2, Side::Buy),
			(BOB, ASSET_1, Side::Sell),
		] {
			const AMOUNT: AssetAmount = 1000;
			MockBalance::credit_account(&lp, base_asset, AMOUNT);
			MockBalance::credit_account(&lp, STABLE_ASSET, AMOUNT);

			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(lp),
				base_asset,
				STABLE_ASSET,
				side,
				ORDER_ID,
				Some(100),
				AMOUNT,
				None,
				None,
			));
		}

		let count_orders = |base_asset, lp| {
			let orders =
				LiquidityPools::pool_orders(base_asset, STABLE_ASSET, &BTreeSet::from([lp]), false)
					.unwrap()
					.limit_orders;

			(orders.asks.len(), orders.bids.len())
		};

		// Alice has two orders in asset_1 (in each direction) and one order in asset 2,
		// Bob also has one order:
		assert_eq!(count_orders(ASSET_1, ALICE), (1, 1));
		assert_eq!(count_orders(ASSET_2, ALICE), (0, 1));
		assert_eq!(count_orders(ASSET_1, BOB), (1, 0));

		assert_ok!(LiquidityPools::cancel_all_limit_orders(&ALICE));

		// All Alice's orders must be closed, Bob's order is untouched:
		assert_eq!(count_orders(ASSET_1, ALICE), (0, 0));
		assert_eq!(count_orders(ASSET_2, ALICE), (0, 0));
		assert_eq!(count_orders(ASSET_1, BOB), (1, 0));
	});
}
#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_LIMIT_ORDER_THRESHOLD_USDC: AssetAmount = 5_000 * 10u128.pow(6);
		const NEW_LIMIT_ORDER_THRESHOLD_USDT: AssetAmount = 6_000 * 10u128.pow(6);

		// Check that the default values are different from the new ones
		assert_ne!(
			LimitOrderAutoSweepingThresholds::<Test>::get()
				.get(&Asset::Usdc)
				.copied()
				.unwrap_or_default(),
			NEW_LIMIT_ORDER_THRESHOLD_USDC
		);
		assert_ne!(
			LimitOrderAutoSweepingThresholds::<Test>::get()
				.get(&Asset::Usdt)
				.copied()
				.unwrap_or_default(),
			NEW_LIMIT_ORDER_THRESHOLD_USDT
		);

		// Update all config items at the same time
		assert_ok!(LiquidityPools::update_pallet_config(
			RuntimeOrigin::root(),
			vec![
				PalletConfigUpdate::LimitOrderAutoSweepingThreshold {
					asset: Asset::Usdc,
					amount: NEW_LIMIT_ORDER_THRESHOLD_USDC
				},
				PalletConfigUpdate::LimitOrderAutoSweepingThreshold {
					asset: Asset::Usdt,
					amount: NEW_LIMIT_ORDER_THRESHOLD_USDT
				},
			]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(
			LimitOrderAutoSweepingThresholds::<Test>::get()
				.get(&Asset::Usdc)
				.copied()
				.unwrap_or_default(),
			NEW_LIMIT_ORDER_THRESHOLD_USDC
		);
		assert_eq!(
			LimitOrderAutoSweepingThresholds::<Test>::get()
				.get(&Asset::Usdt)
				.copied()
				.unwrap_or_default(),
			NEW_LIMIT_ORDER_THRESHOLD_USDT
		);

		// Check that the events were emitted
		assert_events_eq!(
			Test,
			RuntimeEvent::LiquidityPools(Event::PalletConfigUpdated {
				update: PalletConfigUpdate::LimitOrderAutoSweepingThreshold {
					asset: Asset::Usdc,
					amount: NEW_LIMIT_ORDER_THRESHOLD_USDC,
				},
			}),
			RuntimeEvent::LiquidityPools(Event::PalletConfigUpdated {
				update: PalletConfigUpdate::LimitOrderAutoSweepingThreshold {
					asset: Asset::Usdt,
					amount: NEW_LIMIT_ORDER_THRESHOLD_USDT,
				},
			}),
		);

		// Make sure that only governance can update the config
		assert_noop!(
			LiquidityPools::update_pallet_config(
				RuntimeOrigin::signed(ALICE),
				vec![].try_into().unwrap()
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn test_sweeping_when_updating_limit_order() {
	const ASSET: Asset = Asset::Flip;

	let get_balance =
		|lp| (MockBalance::get_balance(lp, ASSET), MockBalance::get_balance(lp, STABLE_ASSET));

	new_test_ext().execute_with(|| {
		// Turn off auto-sweeping
		LimitOrderAutoSweepingThresholds::<Test>::mutate(|thresholds| {
			thresholds.try_insert(ASSET, u128::MAX).unwrap();
			thresholds.try_insert(STABLE_ASSET, u128::MAX).unwrap();
		});

		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			ASSET,
			STABLE_ASSET,
			0, // no fee
			Price::at_tick_zero(),
		));

		// Setup limit orders
		for (lp, amount) in [(ALICE, 10_000), (BOB, 10_000)] {
			MockBalance::credit_account(&lp, ASSET, amount);
			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(lp),
				ASSET,
				STABLE_ASSET,
				Side::Sell,
				1,
				Some(0),
				amount,
				None,
				None,
			));
			MockBalance::credit_account(&lp, STABLE_ASSET, amount);
			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(lp),
				ASSET,
				STABLE_ASSET,
				Side::Buy,
				2,
				Some(0),
				amount,
				None,
				None,
			));
		}
		assert_eq!(get_balance(&ALICE), (0, 0));
		assert_eq!(get_balance(&BOB), (0, 0));

		// Do a swap in each direction to execute the orders
		assert!(LiquidityPools::swap_single_leg(STABLE_ASSET, ASSET, 10_000).is_ok());
		assert!(LiquidityPools::swap_single_leg(ASSET, STABLE_ASSET, 10_000).is_ok());

		// Confirm no sweeping has happened yet
		assert_eq!(get_balance(&ALICE), (0, 0));
		assert_eq!(get_balance(&BOB), (0, 0));
		assert_eq!(HistoricalEarnedFees::<Test>::get(ALICE, STABLE_ASSET), 0);
		assert_eq!(HistoricalEarnedFees::<Test>::get(BOB, STABLE_ASSET), 0);

		// Increase a limit order should cause sweeping of all orders for that LP
		// NOTE: We would not be able to increase this order if Alice's other order was not swept
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			ASSET,
			STABLE_ASSET,
			Side::Sell,
			1,
			None,
			10_000,
			None,
			None,
		));
		assert_eq!(get_balance(&ALICE), (0, 5000));

		// Bob's orders are not swept yet
		assert_eq!(get_balance(&BOB), (0, 0));
		assert_eq!(HistoricalEarnedFees::<Test>::get(BOB, STABLE_ASSET), 0);

		// Decrease a limit order should also cause sweeping for that LP
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			ASSET,
			STABLE_ASSET,
			Side::Sell,
			1,
			None,
			1000,
			None,
			None,
		));
		assert_eq!(get_balance(&BOB), (9000, 5000));
	});
}

#[test]
fn test_sweeping_when_updating_range_order() {
	const ASSET: Asset = Asset::Flip;

	let get_balance =
		|lp| (MockBalance::get_balance(lp, ASSET), MockBalance::get_balance(lp, STABLE_ASSET));

	new_test_ext().execute_with(|| {
		// Turn off auto-sweeping
		LimitOrderAutoSweepingThresholds::<Test>::mutate(|thresholds| {
			thresholds.try_insert(ASSET, u128::MAX).unwrap();
			thresholds.try_insert(STABLE_ASSET, u128::MAX).unwrap();
		});

		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			ASSET,
			STABLE_ASSET,
			10_000, // 100bps fee
			Price::at_tick_zero(),
		));

		// Setup range orders
		for (lp, amount) in [(ALICE, 5_000), (BOB, 5_000)] {
			MockBalance::credit_account(&lp, ASSET, amount * 2);
			MockBalance::credit_account(&lp, STABLE_ASSET, amount * 2);
			for id in 1..=2 {
				assert_ok!(LiquidityPools::set_range_order(
					RuntimeOrigin::signed(lp),
					ASSET,
					STABLE_ASSET,
					id,
					Some(-1..1),
					RangeOrderSize::AssetAmounts {
						minimum: PoolPairsMap { base: 0, quote: 0 },
						maximum: PoolPairsMap { base: amount, quote: amount }
					}
				));
			}
		}
		assert_eq!(get_balance(&ALICE), (0, 0));
		assert_eq!(get_balance(&BOB), (0, 0));

		// Do a swap
		assert!(LiquidityPools::swap_single_leg(STABLE_ASSET, ASSET, 10_000).is_ok());
		assert!(LiquidityPools::swap_single_leg(ASSET, STABLE_ASSET, 10_000).is_ok());

		// Confirm no sweeping has happened yet
		assert_eq!(get_balance(&ALICE), (0, 0));
		assert_eq!(get_balance(&BOB), (0, 0));

		// Increase the range order should cause fee collection of both orders for that LP
		MockBalance::credit_account(&ALICE, STABLE_ASSET, 5_000);
		MockBalance::credit_account(&ALICE, ASSET, 5_000);
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			ASSET,
			STABLE_ASSET,
			1,
			None,
			RangeOrderSize::AssetAmounts {
				minimum: PoolPairsMap { base: 0, quote: 0 },
				maximum: PoolPairsMap { base: 10_000, quote: 10_000 }
			}
		));
		// NOTE: The collected fees would not be correct unless both orders got swept
		const EXPECTED_FEES: u128 = 48;
		assert_eq!(HistoricalEarnedFees::<Test>::get(ALICE, STABLE_ASSET), EXPECTED_FEES);
		// But Bob's orders are not swept yet
		assert_eq!(HistoricalEarnedFees::<Test>::get(BOB, STABLE_ASSET), 0);

		// Decrease the range order should cause sweeping of it
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(BOB),
			ASSET,
			STABLE_ASSET,
			1,
			None,
			RangeOrderSize::Liquidity { liquidity: 100 }
		));
		assert_eq!(HistoricalEarnedFees::<Test>::get(BOB, STABLE_ASSET), EXPECTED_FEES);
	});
}

#[test]
fn test_limit_order_auto_close() {
	const ASSET: Asset = Asset::Flip;
	const CLOSE_ORDER_AT: u64 = 10;
	const DISPATCH_AT: u64 = 5;
	const AMOUNT: AssetAmount = 10_000;
	const ORDER_ID: u64 = 1;

	new_test_ext()
		.execute_with(|| {
			assert_ok!(LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				ASSET,
				STABLE_ASSET,
				0,
				Price::at_tick_zero(),
			));

			MockBalance::credit_account(&ALICE, ASSET, AMOUNT);

			// Make sure that a close order at block that is in the past is not accepted
			assert_noop!(
				LiquidityPools::set_limit_order(
					RuntimeOrigin::signed(ALICE),
					ASSET,
					STABLE_ASSET,
					Side::Sell,
					ORDER_ID,
					Some(5),
					AMOUNT,
					None,
					Some(0), // Block 0 will always be in the past
				),
				Error::<Test>::InvalidCloseOrderAt
			);

			// Test close order block validation
			assert_noop!(
				LiquidityPools::set_limit_order(
					RuntimeOrigin::signed(ALICE),
					ASSET,
					STABLE_ASSET,
					Side::Sell,
					ORDER_ID,
					Some(5),
					AMOUNT,
					// Schedule the call for the same block as the close order, so it
					// should be rejected
					Some(CLOSE_ORDER_AT),
					Some(CLOSE_ORDER_AT),
				),
				Error::<Test>::InvalidCloseOrderAt
			);
			assert_noop!(
				LiquidityPools::set_limit_order(
					RuntimeOrigin::signed(ALICE),
					ASSET,
					STABLE_ASSET,
					Side::Sell,
					ORDER_ID,
					Some(5),
					AMOUNT,
					// Schedule the call for after the close order, so it should be rejected
					Some(CLOSE_ORDER_AT + 1),
					Some(CLOSE_ORDER_AT),
				),
				Error::<Test>::InvalidCloseOrderAt
			);
			assert_noop!(
				LiquidityPools::set_limit_order(
					RuntimeOrigin::signed(ALICE),
					ASSET,
					STABLE_ASSET,
					Side::Sell,
					ORDER_ID,
					Some(5),
					AMOUNT,
					Some(DISPATCH_AT),
					// Schedule the close order for too far in the future
					Some((SCHEDULE_UPDATE_LIMIT_BLOCKS as u64) + 2),
				),
				Error::<Test>::InvalidCloseOrderAt
			);

			// Set a limit order with an expiry block in the future
			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(ALICE),
				ASSET,
				STABLE_ASSET,
				Side::Sell,
				ORDER_ID,
				Some(5),
				AMOUNT,
				Some(DISPATCH_AT),
				Some(CLOSE_ORDER_AT),
			));

			// Check that the event for scheduling the opening of the order is emitted
			assert_events_match!(
				Test,
				RuntimeEvent::LiquidityPools(Event::LimitOrderSetOrUpdateScheduled {
					lp: ALICE,
					order_id: ORDER_ID,
					dispatch_at: DISPATCH_AT,
				}) => ()
			);
		})
		.then_process_blocks_until_block(DISPATCH_AT)
		.then_execute_with(|_| {
			// Check that the event for scheduling the expire order is emitted
			assert_events_match!(
				Test,
				RuntimeEvent::LiquidityPools(Event::LimitOrderSetOrUpdateScheduled {
					lp: ALICE,
					order_id: ORDER_ID,
					dispatch_at: CLOSE_ORDER_AT,
				}) => ()
			);

			// The order should be present in the pool
			assert_eq!(
				LiquidityPools::pool_orders(ASSET, STABLE_ASSET, &BTreeSet::from([ALICE]), false)
					.unwrap()
					.limit_orders
					.asks
					.len(),
				1
			);
		})
		.then_process_blocks_until_block(CLOSE_ORDER_AT)
		.then_execute_with(|_| {
			// The order should be removed after the expiry block
			assert_events_match!(
				Test,
				RuntimeEvent::LiquidityPools(Event::ScheduledLimitOrderUpdateDispatchSuccess {
					lp: ALICE,
					order_id: ORDER_ID,
				}) => ()
			);
			assert_eq!(
				LiquidityPools::pool_orders(ASSET, STABLE_ASSET, &BTreeSet::from([ALICE]), false)
					.unwrap()
					.limit_orders
					.asks
					.len(),
				0
			);
		});
}

#[test]
fn cancel_all_pool_positions() {
	const BASE_ASSET: Asset = Asset::Dot;
	const ORDER_ID: u64 = 1;

	new_test_ext().execute_with(|| {
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			BASE_ASSET,
			STABLE_ASSET,
			0,
			Price::at_tick_zero(),
		));

		for (lp, side, n) in [(ALICE, Side::Sell, 2), (ALICE, Side::Buy, 2), (BOB, Side::Sell, 8)] {
			const AMOUNT: AssetAmount = 1_000_000;
			MockBalance::credit_account(&lp, BASE_ASSET, AMOUNT);
			MockBalance::credit_account(&lp, STABLE_ASSET, AMOUNT);

			// Create multiple orders for each LP
			for i in 0..n {
				assert_ok!(LiquidityPools::set_limit_order(
					RuntimeOrigin::signed(lp),
					BASE_ASSET,
					STABLE_ASSET,
					side,
					ORDER_ID + i as u64,
					Some(100),
					5_000,
					None,
					None,
				));
			}

			// Create a range order for each LP
			for i in 0..n {
				assert_ok!(LiquidityPools::set_range_order(
					RuntimeOrigin::signed(lp),
					BASE_ASSET,
					STABLE_ASSET,
					ORDER_ID + i as u64,
					Some(-10000..10000),
					RangeOrderSize::Liquidity { liquidity: 1_000 },
				));
			}
		}

		let count_orders = |base_asset, lp| {
			let limit_orders =
				LiquidityPools::pool_orders(base_asset, STABLE_ASSET, &BTreeSet::from([lp]), false)
					.unwrap()
					.limit_orders;

			let range_orders =
				LiquidityPools::pool_orders(base_asset, STABLE_ASSET, &BTreeSet::from([lp]), false)
					.unwrap()
					.range_orders;

			(limit_orders.asks.len(), limit_orders.bids.len(), range_orders.len())
		};

		// Alice has 2 limit orders in each direction and 2 range orders.
		// Bob has 8 sell limit orders, 8 range orders.
		assert_eq!(count_orders(BASE_ASSET, ALICE), (2, 2, 2));
		assert_eq!(count_orders(BASE_ASSET, BOB), (8, 0, 8));

		assert_ok!(LiquidityPools::cancel_all_pool_orders(BASE_ASSET, STABLE_ASSET));

		// All orders in the pool should be closed
		assert_eq!(count_orders(BASE_ASSET, ALICE), (0, 0, 0));
		assert_eq!(count_orders(BASE_ASSET, BOB), (0, 0, 0));
	});
}

#[test]
fn test_get_limit_orders() {
	const ASSET: Asset = Asset::Flip;
	const AMOUNT: AssetAmount = 100_000;

	new_test_ext().execute_with(|| {
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			ASSET,
			STABLE_ASSET,
			Default::default(),
			Price::at_tick_zero(),
		));

		MockBalance::credit_account(&ALICE, ASSET, 1_000_000);
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			ASSET,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(0),
			AMOUNT,
			None,
			None,
		));

		let orders =
			LiquidityPools::limit_orders(ASSET, STABLE_ASSET, &BTreeSet::from([ALICE])).unwrap();
		assert_eq!(
			orders,
			vec![cf_amm::common::LimitOrder {
				base_asset: ASSET,
				account_id: ALICE,
				side: Side::Sell,
				order_id: 0,
				tick: 0,
				amount: AMOUNT,
			}]
		);
	});
}
