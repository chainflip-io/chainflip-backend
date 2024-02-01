use crate::{
	self as pallet_cf_pools, mock::*, utilities, AskBidMap, AssetAmounts, AssetPair, AssetsMap,
	CollectedNetworkFee, Error, Event, FlipBuyInterval, FlipToBurn, LimitOrder, PoolInfo,
	PoolOrders, Pools, RangeOrder, RangeOrderSize, ScheduledLimitOrderUpdates, STABLE_ASSET,
};
use cf_amm::common::{price_at_tick, tick_at_price, Order, Tick, PRICE_FRACTIONAL_BITS};
use cf_primitives::{chains::assets::any::Asset, AssetAmount, SwapOutput};
use cf_test_utilities::{assert_events_match, assert_has_event, last_event};
use cf_traits::AssetConverter;
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::U256;
use sp_runtime::Permill;

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
fn test_buy_back_flip_no_funds_available() {
	new_test_ext().execute_with(|| {
		let unstable_asset = Asset::Eth;
		let default_price = price_at_tick(0).unwrap();

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			STABLE_ASSET,
			500_000u32,
			default_price,
		));

		FlipBuyInterval::<Test>::set(5);
		CollectedNetworkFee::<Test>::set(30);
		LiquidityPools::on_initialize(8);
		assert_eq!(FlipToBurn::<Test>::get(), 0);
	});
}

#[test]
fn test_buy_back_flip_2() {
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
			Order::Buy,
			0,
			Some(TICK),
			POSITION_0_SIZE,
		));

		assert_eq!(AliceCollectedEth::get(), 0);
		assert_eq!(AliceCollectedUsdc::get(), 0);
		assert_eq!(AliceDebitedEth::get(), 0);
		assert_eq!(AliceDebitedUsdc::get(), POSITION_0_SIZE);

		LiquidityPools::swap_with_network_fee(ETH, STABLE_ASSET, SWAP_AMOUNT).unwrap();

		assert_eq!(AliceCollectedEth::get(), 0);
		assert_eq!(AliceCollectedUsdc::get(), 0);
		assert_eq!(AliceDebitedEth::get(), 0);
		assert_eq!(AliceDebitedUsdc::get(), POSITION_0_SIZE);

		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			ETH,
			STABLE_ASSET,
			Order::Sell,
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
fn test_buy_back_flip() {
	new_test_ext().execute_with(|| {
		const INTERVAL: BlockNumberFor<Test> = 5;
		const FLIP_PRICE_IN_USDC: u128 = 10;
		const FLIP: Asset = Asset::Flip;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			FLIP,
			STABLE_ASSET,
			Default::default(),
			price_at_tick(0).unwrap(),
		));
		for side in [Order::Buy, Order::Sell] {
			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(ALICE),
				FLIP,
				STABLE_ASSET,
				side,
				0,
				Some(
					tick_at_price(U256::from(FLIP_PRICE_IN_USDC) << PRICE_FRACTIONAL_BITS).unwrap()
				),
				1_000_000_000,
			));
		}

		// Swapping should cause the network fee to be collected.
		// Do two swaps of equivalent value.
		const USDC_SWAP_VALUE: u128 = 100_000;
		const FLIP_SWAP_VALUE: u128 = USDC_SWAP_VALUE / FLIP_PRICE_IN_USDC;
		LiquidityPools::swap_with_network_fee(FLIP, STABLE_ASSET, FLIP_SWAP_VALUE).unwrap();
		LiquidityPools::swap_with_network_fee(STABLE_ASSET, FLIP, USDC_SWAP_VALUE).unwrap();

		// 2 swaps of 100_000 USDC, 0.2% fee
		const EXPECTED_COLLECTED_FEES: AssetAmount = 400;
		assert_eq!(CollectedNetworkFee::<Test>::get(), EXPECTED_COLLECTED_FEES);

		// The default buy interval is zero, and this means we don't buy back.
		assert_eq!(FlipBuyInterval::<Test>::get(), 0);
		LiquidityPools::on_initialize(1);
		assert_eq!(FlipToBurn::<Test>::get(), 0);

		// A non-zero buy interval
		FlipBuyInterval::<Test>::set(INTERVAL);

		// Nothing is bought if we're not at the interval.
		LiquidityPools::on_initialize(INTERVAL * 3 - 1);
		assert_eq!(0, FlipToBurn::<Test>::get());
		assert_eq!(EXPECTED_COLLECTED_FEES, CollectedNetworkFee::<Test>::get());

		// If we're at an interval, we should buy flip.
		LiquidityPools::on_initialize(INTERVAL * 3);
		assert_eq!(0, CollectedNetworkFee::<Test>::get());
		assert!(
			FlipToBurn::<Test>::get().abs_diff(EXPECTED_COLLECTED_FEES / FLIP_PRICE_IN_USDC) <= 1
		);
	});
}

#[test]
fn test_network_fee_calculation() {
	new_test_ext().execute_with(|| {
		// Show we can never overflow and panic
		utilities::calculate_network_fee(Permill::from_percent(100), AssetAmount::MAX);
		// 200 bps (2%) of 100 = 2
		assert_eq!(utilities::calculate_network_fee(Permill::from_percent(2u32), 100), (98, 2));
		// 2220 bps = 22 % of 199 = 43,78
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), 199),
			(155, 44)
		);
		// 2220 bps = 22 % of 234 = 51,26
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), 233),
			(181, 52)
		);
		// 10 bps = 0,1% of 3000 = 3
		assert_eq!(
			utilities::calculate_network_fee(Permill::from_rational(1u32, 1000u32), 3000),
			(2997, 3)
		);
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
			})
		);

		// Setup liquidity for the pool with 2 LPer
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Order::Sell,
			0,
			Some(0),
			5_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Order::Buy,
			1,
			Some(0),
			1_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Order::Sell,
			0,
			Some(0),
			10_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Order::Buy,
			1,
			Some(0),
			10_000,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE)),
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
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(BOB)),
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
		assert_eq!(
			LiquidityPools::swap_with_network_fee(STABLE_ASSET, Asset::Eth, 10_000).unwrap(),
			SwapOutput { intermediary: None, output: 5_987u128 }
		);
		assert_eq!(
			LiquidityPools::swap_with_network_fee(Asset::Eth, STABLE_ASSET, 10_000).unwrap(),
			SwapOutput { intermediary: None, output: 5_987u128 }
		);

		// Updates the fees to the new value and collect any fees on current positions.
		assert_ok!(LiquidityPools::set_pool_fees(
			RuntimeOrigin::root(),
			Asset::Eth,
			STABLE_ASSET,
			new_fee
		));

		// All Lpers' fees and bought amount are Collected and accredited.
		// Fee and swaps are calculated proportional to the liquidity amount.
		assert_eq!(AliceCollectedEth::get(), 908u128);
		assert_eq!(AliceCollectedUsdc::get(), 3_325u128);
		assert_eq!(BobCollectedEth::get(), 9090u128);
		assert_eq!(BobCollectedUsdc::get(), 6_651u128);

		// New pool fee is set and event emitted.
		assert_eq!(
			LiquidityPools::pool_info(Asset::Eth, STABLE_ASSET),
			Ok(PoolInfo {
				limit_order_fee_hundredth_pips: new_fee,
				range_order_fee_hundredth_pips: new_fee,
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
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE)),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: ALICE,
						id: 0.into(),
						tick: 0,
						sell_amount: 3004.into(),
						fees_earned: 1330.into(),
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
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(BOB)),
			Ok(PoolOrders {
				limit_orders: AskBidMap {
					asks: vec![LimitOrder {
						lp: BOB,
						id: 0.into(),
						tick: 0,
						sell_amount: 6_008u128.into(),
						fees_earned: 2660.into(),
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
			Order::Sell,
			0,
			Some(0),
			100,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Order::Sell,
			0,
			Some(tick),
			100_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			Order::Buy,
			1,
			Some(tick),
			10_000,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE)),
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
		assert_eq!(
			LiquidityPools::swap_with_network_fee(STABLE_ASSET, Asset::Eth, 202_200).unwrap(),
			SwapOutput { intermediary: None, output: 99_894u128 }
		);
		assert_eq!(
			LiquidityPools::swap_with_network_fee(Asset::Eth, STABLE_ASSET, 18_000).unwrap(),
			SwapOutput { intermediary: None, output: 9_071 }
		);

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
			side: Order::Sell,
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
			side: Order::Sell,
			id: 0,
			tick: 100,
			sell_amount_change: None,
			sell_amount_total: 205,
			collected_fees: 100796,
			bought_amount: 100796,
		}));
		assert_has_event::<Test>(RuntimeEvent::LiquidityPools(Event::<Test>::LimitOrderUpdated {
			lp: BOB,
			base_asset: Asset::Eth,
			quote_asset: STABLE_ASSET,
			side: Order::Buy,
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
		assert_eq!(
			LiquidityPools::swap_with_network_fee(STABLE_ASSET, Asset::Eth, 5_000).unwrap(),
			SwapOutput { intermediary: None, output: 2_989u128 }
		);
		assert_eq!(
			LiquidityPools::swap_with_network_fee(Asset::Eth, STABLE_ASSET, 5_000).unwrap(),
			SwapOutput { intermediary: None, output: 2_998u128 }
		);

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
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(ALICE)),
			Ok(PoolOrders {
				limit_orders: AskBidMap { asks: vec![], bids: vec![] },
				range_orders: vec![RangeOrder {
					lp: ALICE,
					id: 0.into(),
					range: range.clone(),
					liquidity: 1_000_000,
					fees_earned: AssetsMap { base: 999.into(), quote: 997.into() }
				}]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, Some(BOB)),
			Ok(PoolOrders {
				limit_orders: AskBidMap { asks: vec![], bids: vec![] },
				range_orders: vec![RangeOrder {
					lp: BOB,
					id: 0.into(),
					range: range.clone(),
					liquidity: 1_000_000,
					fees_earned: AssetsMap { base: 999.into(), quote: 997.into() }
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
		assert_eq!(AliceCollectedEth::get(), 5_991u128);
		assert_eq!(AliceCollectedUsdc::get(), 5_979u128);
		assert_eq!(AliceDebitedEth::get(), 4_988u128);
		assert_eq!(AliceDebitedUsdc::get(), 4_988u128);

		assert_eq!(BobCollectedEth::get(), 5_991u128);
		assert_eq!(BobCollectedUsdc::get(), 5_979u128);
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
				side: Order::Buy,
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
					side: Order::Buy,
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
			Order::Sell,
			4,
			Some(100),
			500_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Order::Sell,
			5,
			Some(1000),
			600_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Order::Sell,
			6,
			Some(100),
			700_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			Order::Buy,
			7,
			Some(1000),
			800_000,
		));

		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, None),
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
fn asset_conversion() {
	new_test_ext().execute_with(|| {
		// Create pools
		for asset in [Asset::Eth, Asset::Flip] {
			assert_ok!(LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				asset,
				STABLE_ASSET,
				Default::default(),
				price_at_tick(0).unwrap(),
			));
			assert_ok!(LiquidityPools::set_range_order(
				RuntimeOrigin::signed(ALICE),
				asset,
				STABLE_ASSET,
				0,
				Some(-100..100),
				RangeOrderSize::Liquidity { liquidity: 100_000_000_000_000_000 },
			));
		}

		const AVAILABLE: AssetAmount = 1_000_000;
		const DESIRED: AssetAmount = 10_000;
		// No available funds -> no conversion.
		assert!(LiquidityPools::convert_asset_to_approximate_output(
			Asset::Flip,
			0u128,
			Asset::Eth,
			DESIRED,
		)
		.is_none());

		// Desired output is zero -> trivially ok.
		assert_eq!(
			LiquidityPools::convert_asset_to_approximate_output(
				Asset::Flip,
				AVAILABLE,
				Asset::Eth,
				0u128,
			),
			Some((AVAILABLE, 0))
		);

		// Desired output is available -> assets converted.
		assert!(matches!(
			LiquidityPools::convert_asset_to_approximate_output(
				Asset::Flip,
				AVAILABLE,
				Asset::Eth,
				DESIRED,
			),
			Some((remaining, converted)) if converted > 0 && remaining + converted <= AVAILABLE
		),);
		cf_test_utilities::assert_event_sequence!(
			Test,
			RuntimeEvent::LiquidityPools(Event::NewPoolCreated { .. }),
			RuntimeEvent::LiquidityPools(Event::RangeOrderUpdated { .. }),
			RuntimeEvent::LiquidityPools(Event::NewPoolCreated { .. }),
			RuntimeEvent::LiquidityPools(Event::RangeOrderUpdated { .. }),
			RuntimeEvent::LiquidityPools(Event::AssetSwapped {
				from: Asset::Flip,
				to: STABLE_ASSET,
				..
			}),
			RuntimeEvent::LiquidityPools(Event::NetworkFeeTaken { .. }),
			RuntimeEvent::LiquidityPools(Event::AssetSwapped {
				from: STABLE_ASSET,
				to: Asset::Eth,
				..
			}),
		);
	});
}
