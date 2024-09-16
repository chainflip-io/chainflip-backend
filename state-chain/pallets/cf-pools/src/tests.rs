use crate::{
	self as pallet_cf_pools, mock::*, AskBidMap, AssetAmounts, AssetPair, CloseOrder, Error, Event,
	HistoricalEarnedFees, LimitOrder, PoolInfo, PoolOrders, PoolPairsMap, Pools, RangeOrder,
	RangeOrderSize, ScheduledLimitOrderUpdates, STABLE_ASSET,
};
use cf_amm::common::{price_at_tick, Side, Tick};
use cf_primitives::{chains::assets::any::Asset, AssetAmount};
use cf_test_utilities::{assert_events_match, assert_has_event, last_event};
use cf_traits::{PoolApi, SwappingApi};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_core::{bounded_vec, ConstU32, U256};
use sp_runtime::BoundedVec;

#[test]
fn can_create_new_trading_pool() {
	new_test_ext().execute_with(|| {
		let unstable_asset = Asset::Eth;
		let default_price = price_at_tick(0).unwrap();

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
			price_at_tick(0).unwrap(),
		));
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
fn test_sweeping() {
	new_test_ext().execute_with(|| {
		const TICK: Tick = 0;
		const ETH: Asset = Asset::Eth;
		const POSITION_0_SIZE: AssetAmount = 100_000;
		const POSITION_1_SIZE: AssetAmount = 90_000;
		const SWAP_AMOUNT: AssetAmount = 50_000;

		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			ETH,
			STABLE_ASSET,
			Default::default(),
			price_at_tick(0).unwrap(),
		));

		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			ETH,
			STABLE_ASSET,
			Side::Buy,
			0,
			Some(TICK),
			POSITION_0_SIZE,
		));

		assert_eq!(AliceCollectedEth::get(), 0);
		assert_eq!(AliceCollectedUsdc::get(), 0);
		assert_eq!(AliceDebitedEth::get(), 0);
		assert_eq!(AliceDebitedUsdc::get(), POSITION_0_SIZE);

		LiquidityPools::swap_single_leg(ETH, STABLE_ASSET, SWAP_AMOUNT).unwrap();

		assert_eq!(AliceCollectedEth::get(), 0);
		assert_eq!(AliceCollectedUsdc::get(), 0);
		assert_eq!(AliceDebitedEth::get(), 0);
		assert_eq!(AliceDebitedUsdc::get(), POSITION_0_SIZE);

		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			ETH,
			STABLE_ASSET,
			Side::Sell,
			1,
			Some(TICK),
			POSITION_1_SIZE,
		));

		assert_eq!(AliceCollectedEth::get(), SWAP_AMOUNT);
		assert_eq!(AliceCollectedUsdc::get(), 0);
		assert_eq!(AliceDebitedEth::get(), POSITION_1_SIZE);
		assert_eq!(AliceDebitedUsdc::get(), POSITION_0_SIZE);
	});
}

#[test]
fn can_update_pool_liquidity_fee_and_collect_for_limit_order() {
	new_test_ext().execute_with(|| {
		let old_fee = 400_000u32;
		let new_fee = 100_000u32;
		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			old_fee,
			price_at_tick(0).unwrap(),
		));
		assert_eq!(
			LiquidityPools::pool_info(Asset::Eth, STABLE_ASSET),
			Ok(PoolInfo {
				limit_order_fee_hundredth_pips: old_fee,
				range_order_fee_hundredth_pips: old_fee,
				range_order_total_fees_earned: Default::default(),
				limit_order_total_fees_earned: Default::default(),
				range_total_swap_inputs: Default::default(),
				limit_total_swap_inputs: Default::default(),
			})
		);

		// Setup liquidity for the pool with 2 LPer
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(0),
			5_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Buy,
			1,
			Some(0),
			1_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(0),
			10_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Side::Buy,
			1,
			Some(0),
			10_000,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: ALICE,
						id: 0.into(),
						tick: 0,
						sell_amount: 5000u128.into(),
						fees_earned: 0.into(),
						original_sell_amount: 5000u128.into()
					}],
					bids: vec![LimitOrder {
						lp: ALICE,
						id: 1.into(),
						tick: 0,
						sell_amount: 1000.into(),
						fees_earned: 0.into(),
						original_sell_amount: 1000u128.into()
					}]
				},
				range_orders: vec![]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(BOB), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: BOB,
						id: 0.into(),
						tick: 0,
						sell_amount: 10000u128.into(),
						fees_earned: 0.into(),
						original_sell_amount: 10000u128.into()
					}],
					bids: vec![LimitOrder {
						lp: BOB,
						id: 1.into(),
						tick: 0,
						sell_amount: 10000.into(),
						fees_earned: 0.into(),
						original_sell_amount: 10000u128.into()
					}]
				},
				range_orders: vec![]
			})
		);

		// Do some swaps to collect fees.
		LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, 10_000).unwrap();
		LiquidityPools::swap_single_leg(Asset::Eth, STABLE_ASSET, 10_000).unwrap();

		// Updates the fees to the new value and collect any fees on current positions.
		assert_ok!(LiquidityPools::set_pool_fees(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			new_fee
		));

		// All LP'ers fees and bought amount are Collected and accredited.
		// Fee and swaps are calculated proportional to the liquidity amount.
		assert_eq!(AliceCollectedEth::get(), 908u128);
		assert_eq!(AliceCollectedUsdc::get(), 3_333u128);
		assert_eq!(BobCollectedEth::get(), 9090u128);
		assert_eq!(BobCollectedUsdc::get(), 6_666u128);

		// New pool fee is set and event emitted.
		assert_eq!(
			LiquidityPools::pool_info(Asset::Eth, STABLE_ASSET),
			Ok(PoolInfo {
				limit_order_fee_hundredth_pips: new_fee,
				range_order_fee_hundredth_pips: new_fee,
				range_order_total_fees_earned: Default::default(),
				limit_order_total_fees_earned: PoolPairsMap {
					base: U256::from(4000),
					quote: U256::from(4000)
				},
				range_total_swap_inputs: Default::default(),
				limit_total_swap_inputs: PoolPairsMap {
					base: U256::from(6000),
					quote: U256::from(6000)
				},
			})
		);

		System::assert_has_event(RuntimeEvent::LiquidityPools(Event::<Test>::PoolFeeSet {
			base_asset: Asset::Eth,
			quote_asset: STABLE_ASSET,
			fee_hundredth_pips: new_fee,
		}));

		// Collected fees and bought amount are reset and position updated.
		// Alice's remaining liquidity = 5_000 - 2_000
		// Bob's remaining liquidity = 10_000 - 4_000
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: ALICE,
						id: 0.into(),
						tick: 0,
						sell_amount: 3000.into(),
						fees_earned: 1333.into(),
						original_sell_amount: 5000.into()
					}],
					bids: vec![LimitOrder {
						lp: ALICE,
						id: 1.into(),
						tick: 0,
						sell_amount: 454.into(),
						fees_earned: 363.into(),
						original_sell_amount: 1000.into()
					}]
				},
				range_orders: vec![]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(BOB), false),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: BOB,
						id: 0.into(),
						tick: 0,
						sell_amount: 6_000u128.into(),
						fees_earned: 2666.into(),
						original_sell_amount: 10000.into()
					}],
					bids: vec![LimitOrder {
						lp: BOB,
						id: 1.into(),
						tick: 0,
						sell_amount: 4_545.into(),
						fees_earned: 3636.into(),
						original_sell_amount: 10000u128.into()
					}]
				},
				range_orders: vec![]
			})
		);

		// Setting the pool fees will collect nothing, since all positions are reset/refreshed.
		AliceCollectedEth::set(0u128);
		AliceCollectedUsdc::set(0u128);
		BobCollectedEth::set(0u128);
		BobCollectedUsdc::set(0u128);
		assert_ok!(LiquidityPools::set_pool_fees(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			new_fee
		));

		// No fees are collected.
		assert_eq!(AliceCollectedEth::get(), 0u128);
		assert_eq!(AliceCollectedUsdc::get(), 0u128);
		assert_eq!(BobCollectedEth::get(), 0u128);
		assert_eq!(BobCollectedUsdc::get(), 0u128);
	});
}

#[test]
fn pallet_limit_order_is_in_sync_with_pool() {
	new_test_ext().execute_with(|| {
		let fee = 500_000u32;
		let tick = 100;
		let asset_pair = AssetPair::new(Asset::Eth, STABLE_ASSET).unwrap();

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			fee,
			price_at_tick(0).unwrap(),
		));

		// Setup liquidity for the pool with 2 LPer
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(0),
			100,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(tick),
			100_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Side::Buy,
			1,
			Some(tick),
			10_000,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE), false),
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

		// Do some swaps to collect fees.
		LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, 202_200).unwrap();
		LiquidityPools::swap_single_leg(Asset::Eth, STABLE_ASSET, 18_000).unwrap();

		// Updates the fees to the new value and collect any fees on current positions.
		assert_ok!(LiquidityPools::set_pool_fees(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			0u32
		));

		// 100 swapped + 100 fee. The position is fully consumed.
		assert_eq!(AliceCollectedUsdc::get(), 200u128);
		assert_eq!(AliceDebitedEth::get(), 100u128);
		let pallet_limit_orders = Pools::<Test>::get(asset_pair).unwrap().limit_orders_cache;
		assert_eq!(pallet_limit_orders.base.get(&ALICE), None);
		assert_eq!(pallet_limit_orders.base.get(&BOB).unwrap().get(&0), Some(&100));

		assert_has_event::<Test>(RuntimeEvent::LiquidityPools(Event::<Test>::LimitOrderUpdated {
			lp: ALICE,
			base_asset: Asset::Eth,
			quote_asset: STABLE_ASSET,
			side: Side::Sell,
			id: 0,
			tick: 0,
			sell_amount_change: None,
			sell_amount_total: 0,
			collected_fees: 100,
			bought_amount: 100,
		}));
		assert_has_event::<Test>(RuntimeEvent::LiquidityPools(Event::<Test>::LimitOrderUpdated {
			lp: BOB,
			base_asset: Asset::Eth,
			quote_asset: STABLE_ASSET,
			side: Side::Sell,
			id: 0,
			tick: 100,
			sell_amount_change: None,
			sell_amount_total: 5,
			collected_fees: 100998,
			bought_amount: 100998,
		}));
		assert_has_event::<Test>(RuntimeEvent::LiquidityPools(Event::<Test>::LimitOrderUpdated {
			lp: BOB,
			base_asset: Asset::Eth,
			quote_asset: STABLE_ASSET,
			side: Side::Buy,
			id: 1,
			tick: 100,
			sell_amount_change: None,
			sell_amount_total: 910,
			collected_fees: 8998,
			bought_amount: 8998,
		}));
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
			price_at_tick(0).unwrap(),
		));
		assert_eq!(
			LiquidityPools::pool_info(Asset::Eth, STABLE_ASSET),
			Ok(PoolInfo {
				limit_order_fee_hundredth_pips: old_fee,
				range_order_fee_hundredth_pips: old_fee,
				range_order_total_fees_earned: Default::default(),
				limit_order_total_fees_earned: Default::default(),
				range_total_swap_inputs: Default::default(),
				limit_total_swap_inputs: Default::default(),
			})
		);

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
		assert_eq!(AliceCollectedEth::get(), 0u128);
		assert_eq!(AliceCollectedUsdc::get(), 0u128);
		assert_eq!(BobCollectedEth::get(), 0u128);
		assert_eq!(BobCollectedUsdc::get(), 0u128);

		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE), false),
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
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(BOB), false),
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
		assert_eq!(AliceCollectedEth::get(), 5_988u128);
		assert_eq!(AliceCollectedUsdc::get(), 5_984u128);
		assert_eq!(AliceDebitedEth::get(), 4_988u128);
		assert_eq!(AliceDebitedUsdc::get(), 4_988u128);

		assert_eq!(BobCollectedEth::get(), 5_988u128);
		assert_eq!(BobCollectedUsdc::get(), 5_984u128);
		assert_eq!(BobDebitedEth::get(), 4_988u128);
		assert_eq!(BobDebitedUsdc::get(), 4_988u128);
	});
}

#[test]
fn can_execute_scheduled_limit_order() {
	new_test_ext().execute_with(|| {
		let order_id = 0;
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Flip,
			STABLE_ASSET,
			400_000u32,
			price_at_tick(0).unwrap(),
		));
		assert_ok!(LiquidityPools::schedule_limit_order_update(
			RuntimeOrigin::signed(ALICE),
			Box::new(pallet_cf_pools::Call::<Test>::set_limit_order {
				base_asset: Asset::Flip,
				quote_asset: STABLE_ASSET,
				side: Side::Buy,
				id: order_id,
				option_tick: Some(100),
				sell_amount: 55,
			}),
			6
		));
		assert_eq!(
			last_event::<Test>(),
			RuntimeEvent::LiquidityPools(crate::Event::LimitOrderSetOrUpdateScheduled {
				lp: ALICE,
				order_id,
				dispatch_at: 6,
			})
		);
		assert!(!ScheduledLimitOrderUpdates::<Test>::get(6).is_empty());
		LiquidityPools::on_initialize(6);
		assert!(
			ScheduledLimitOrderUpdates::<Test>::get(6).is_empty(),
			"Should be empty, but is {:?}",
			ScheduledLimitOrderUpdates::<Test>::get(6)
		);
		assert_eq!(
			last_event::<Test>(),
			RuntimeEvent::LiquidityPools(crate::Event::ScheduledLimitOrderUpdateDispatchSuccess {
				lp: ALICE,
				order_id,
			})
		);
	});
}

#[test]
fn schedule_rejects_unsupported_calls() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityPools::schedule_limit_order_update(
				RuntimeOrigin::signed(ALICE),
				Box::new(pallet_cf_pools::Call::<Test>::set_pool_fees {
					base_asset: Asset::Eth,
					quote_asset: STABLE_ASSET,
					fee_hundredth_pips: 0,
				}),
				6
			),
			Error::<Test>::UnsupportedCall
		);
	});
}

#[test]
fn cant_schedule_in_the_past() {
	new_test_ext().then_execute_at_block(10u32, |_| {
		assert_noop!(
			LiquidityPools::schedule_limit_order_update(
				RuntimeOrigin::signed(ALICE),
				Box::new(pallet_cf_pools::Call::<Test>::set_limit_order {
					base_asset: Asset::Flip,
					quote_asset: STABLE_ASSET,
					side: Side::Buy,
					id: 0,
					option_tick: Some(0),
					sell_amount: 55,
				}),
				9
			),
			Error::<Test>::LimitOrderUpdateExpired
		);
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
			price_at_tick(0).unwrap(),
		));

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
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			5,
			Some(1000),
			600_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Sell,
			6,
			Some(100),
			700_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Side::Buy,
			7,
			Some(1000),
			800_000,
		));

		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, None, false),
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
fn fees_are_recorded() {
	new_test_ext().execute_with(|| {
		let range_1 = -100..100;

		// Create a new pool.
		for asset in [Asset::Eth, Asset::Btc] {
			assert_ok!(LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				asset,
				STABLE_ASSET,
				100,
				price_at_tick(0).unwrap(),
			));
		}

		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(range_1.clone()),
			RangeOrderSize::Liquidity { liquidity: 1_000_000_000_000_000_000 },
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Btc,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(0),
			1_000_000_000_000_000_000,
		));

		assert!(
			LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Eth, 1_000_000_000).unwrap() > 0
		);
		assert!(
			LiquidityPools::swap_single_leg(STABLE_ASSET, Asset::Btc, 1_000_000_000).unwrap() > 0
		);
		LiquidityPools::sweep(&ALICE).unwrap();
		LiquidityPools::sweep(&BOB).unwrap();

		assert!(
			HistoricalEarnedFees::<Test>::get(ALICE, Asset::Usdc) > 0,
			"Alice's fees should be recorded but are:{:?}",
			HistoricalEarnedFees::<Test>::iter_prefix(ALICE).collect::<Vec<_>>(),
		);
		assert!(
			HistoricalEarnedFees::<Test>::get(BOB, Asset::Usdc) > 0,
			"Bob's fees should be recorded but are:{:?}",
			HistoricalEarnedFees::<Test>::iter_prefix(BOB).collect::<Vec<_>>(),
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
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PriceImpactLimitSet {
				asset_pair: AssetPair::new(OTHER_ASSET, STABLE_ASSET).unwrap(),
				limit: Some(1),
			},
		));

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
					price_at_tick(0).unwrap(),
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
		let default_price = price_at_tick(0).unwrap();

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
			System::assert_last_event(RuntimeEvent::LiquidityPools(
				Event::<Test>::NewPoolCreated {
					base_asset: asset,
					quote_asset: STABLE_ASSET,
					fee_hundredth_pips: 0u32,
					initial_price: default_price,
				},
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
		const POSITION: core::ops::Range<Tick> = -100_000..100_000;
		const FLIP: Asset = Asset::Flip;
		const TICK: Tick = 12;
		let mut orders_to_delete: BoundedVec<CloseOrder, ConstU32<100>> = BoundedVec::new();
		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			FLIP,
			STABLE_ASSET,
			Default::default(),
			price_at_tick(0).unwrap(),
		));
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
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			Side::Sell,
			0,
			Some(TICK),
			12
		));
		assert_events_match!(
			Test,
			RuntimeEvent::LiquidityPools(
				Event::LimitOrderUpdated {
					..
				},
			) => ()
		);
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			STABLE_ASSET,
			Side::Sell,
			1,
			Some(TICK + 1),
			15
		));
		assert_events_match!(
			Test,
			RuntimeEvent::LiquidityPools(
				Event::LimitOrderUpdated {
					..
				},
			) => ()
		);
		orders_to_delete
			.try_push(CloseOrder::Limit {
				base_asset: FLIP,
				quote_asset: STABLE_ASSET,
				side: Side::Sell,
				id: 0,
			})
			.expect("Len should be below 100");
		orders_to_delete
			.try_push(CloseOrder::Limit {
				base_asset: FLIP,
				quote_asset: STABLE_ASSET,
				side: Side::Sell,
				id: 1,
			})
			.expect("Len should be below 100");
		orders_to_delete
			.try_push(CloseOrder::Range { base_asset: FLIP, quote_asset: STABLE_ASSET, id: 0 })
			.expect("Len should be below 100");
		assert_eq!(
			LiquidityPools::open_order_count(
				&ALICE,
				&PoolPairsMap { base: FLIP, quote: STABLE_ASSET }
			)
			.unwrap(),
			3
		);
		assert_ok!(LiquidityPools::cancel_orders_batch(
			RuntimeOrigin::signed(ALICE),
			orders_to_delete
		));
		assert_eq!(
			LiquidityPools::open_order_count(
				&ALICE,
				&PoolPairsMap { base: FLIP, quote: STABLE_ASSET }
			)
			.unwrap(),
			0
		)
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
			price_at_tick(0).unwrap(),
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
