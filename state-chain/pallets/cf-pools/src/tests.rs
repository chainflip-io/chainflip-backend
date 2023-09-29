use crate::{
	mock::*, utilities, AssetAmounts, AssetPair, AssetsMap, CanonicalAssetPair,
	CollectedNetworkFee, Error, FlipBuyInterval, FlipToBurn, PoolInfo, PoolOrders, Pools,
	RangeOrderSize, STABLE_ASSET,
};
use cf_amm::common::{price_at_tick, Tick};
use cf_primitives::{chains::assets::any::Asset, AssetAmount, SwapOutput};
use cf_test_utilities::assert_events_match;
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::Permill;

#[test]
fn can_create_new_trading_pool() {
	new_test_ext().execute_with(|| {
		let unstable_asset = Asset::Eth;
		let default_price = price_at_tick(0).unwrap();

		// While the pool does not exist, no info can be obtained.
		assert!(Pools::<Test>::get(CanonicalAssetPair::new(unstable_asset, STABLE_ASSET).unwrap())
			.is_none());

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
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::NewPoolCreated {
				base_asset: unstable_asset,
				pair_asset: STABLE_ASSET,
				fee_hundredth_pips: 500_000u32,
				initial_price: default_price,
			},
		));

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
fn can_enable_disable_trading_pool() {
	new_test_ext().execute_with(|| {
		let range = -100..100;
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

		// Disable the pool
		assert_ok!(LiquidityPools::update_pool_enabled(
			RuntimeOrigin::root(),
			unstable_asset,
			STABLE_ASSET,
			false
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PoolStateUpdated {
				base_asset: unstable_asset,
				pair_asset: STABLE_ASSET,
				enabled: false,
			},
		));

		assert_noop!(
			LiquidityPools::set_range_order(
				RuntimeOrigin::signed(ALICE),
				Asset::Usdc,
				unstable_asset,
				0,
				Some(range.clone()),
				RangeOrderSize::Liquidity { liquidity: 1_000_000 },
			),
			Error::<Test>::PoolDisabled
		);

		// Re-enable the pool
		assert_ok!(LiquidityPools::update_pool_enabled(
			RuntimeOrigin::root(),
			unstable_asset,
			STABLE_ASSET,
			true
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PoolStateUpdated {
				base_asset: unstable_asset,
				pair_asset: STABLE_ASSET,
				enabled: true,
			},
		));

		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Usdc,
			unstable_asset,
			0,
			Some(range),
			RangeOrderSize::Liquidity { liquidity: 1_000_000 },
		));
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
			STABLE_ASSET,
			FLIP,
			0,
			Some(POSITION),
			RangeOrderSize::AssetAmounts {
				maximum: AssetAmounts { base: 1_000_000, pair: 1_000_000 },
				minimum: AssetAmounts { base: 900_000, pair: 900_000 },
			}
		));
		assert_events_match!(
			Test,
			RuntimeEvent::LiquidityPools(
				crate::Event::RangeOrderUpdated {
					..
				},
			) => ()
		);
		assert_ok!(LiquidityPools::set_range_order(
			RuntimeOrigin::signed(ALICE),
			STABLE_ASSET,
			FLIP,
			0,
			Some(POSITION),
			RangeOrderSize::Liquidity { liquidity: 0 }
		));
	});
}

#[test]
fn test_buy_back_flip() {
	new_test_ext().execute_with(|| {
		const INTERVAL: BlockNumberFor<Test> = 5;
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
			STABLE_ASSET,
			FLIP,
			0,
			Some(POSITION),
			RangeOrderSize::Liquidity { liquidity: 1_000_000 },
		));

		// Swapping should cause the network fee to be collected.
		LiquidityPools::swap_with_network_fee(FLIP, STABLE_ASSET, 1000).unwrap();
		LiquidityPools::swap_with_network_fee(STABLE_ASSET, FLIP, 1000).unwrap();

		let collected_fee = CollectedNetworkFee::<Test>::get();
		assert!(collected_fee > 0);

		// The default buy interval is zero, and this means we don't buy back.
		assert_eq!(FlipBuyInterval::<Test>::get(), 0);
		LiquidityPools::on_initialize(1);
		assert_eq!(FlipToBurn::<Test>::get(), 0);

		// A non-zero buy interval
		FlipBuyInterval::<Test>::set(INTERVAL);

		// Nothing is bought if we're not at the interval.
		LiquidityPools::on_initialize(INTERVAL * 3 - 1);
		assert_eq!(0, FlipToBurn::<Test>::get());
		assert_eq!(collected_fee, CollectedNetworkFee::<Test>::get());

		// If we're at an interval, we should buy flip.
		LiquidityPools::on_initialize(INTERVAL * 3);
		assert_eq!(0, CollectedNetworkFee::<Test>::get());
		assert!(FlipToBurn::<Test>::get() > 0);
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
fn can_update_pool_liquidity_fee() {
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
			Some(PoolInfo {
				limit_order_fee_hundredth_pips: old_fee,
				range_order_fee_hundredth_pips: old_fee,
			})
		);

		// Setup liquidity for the pool with 2 LPer
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(0),
			5_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(ALICE),
			STABLE_ASSET,
			Asset::Eth,
			1,
			Some(0),
			1_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(0),
			10_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			STABLE_ASSET,
			Asset::Eth,
			1,
			Some(0),
			10_000,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &ALICE,),
			Some(PoolOrders {
				limit_orders: AssetsMap {
					base: vec![(0, 0, 5000u128.into())],
					pair: vec![(1, 0, 1000u128.into())]
				},
				range_orders: vec![]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BOB,),
			Some(PoolOrders {
				limit_orders: AssetsMap {
					base: vec![(0, 0, 10_000u128.into())],
					pair: vec![(1, 0, 10_000u128.into())]
				},
				range_orders: vec![]
			})
		);

		// Do some swaps to collect fees.
		assert_eq!(
			LiquidityPools::swap_with_network_fee(STABLE_ASSET, Asset::Eth, 10_000).unwrap(),
			SwapOutput { intermediary: None, output: 5_988u128 }
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
		assert_eq!(AliceCollectedUsdc::get(), 3_333u128);
		assert_eq!(BobCollectedEth::get(), 9090u128);
		assert_eq!(BobCollectedUsdc::get(), 6_666u128);

		// New pool fee is set and event emitted.
		assert_eq!(
			LiquidityPools::pool_info(Asset::Eth, STABLE_ASSET),
			Some(PoolInfo {
				limit_order_fee_hundredth_pips: new_fee,
				range_order_fee_hundredth_pips: new_fee,
			})
		);
		System::assert_has_event(RuntimeEvent::LiquidityPools(crate::Event::<Test>::PoolFeeSet {
			base_asset: Asset::Eth,
			pair_asset: STABLE_ASSET,
			fee_hundredth_pips: new_fee,
		}));

		// Collected fees and bought amount are reset and position updated.
		// Alice's remaining liquidity = 5_000 - 2_000
		// Bob's remaining liquidity = 10_000 - 4_000
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &ALICE,),
			Some(PoolOrders {
				limit_orders: AssetsMap {
					base: vec![(0, 0, 3_000u128.into())],
					pair: vec![(1, 0, 454u128.into())]
				},
				range_orders: vec![]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BOB,),
			Some(PoolOrders {
				limit_orders: AssetsMap {
					base: vec![(0, 0, 6_000u128.into())],
					pair: vec![(1, 0, 4_545u128.into())]
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
		let asset_pair = AssetPair::<Test>::new(Asset::Eth, STABLE_ASSET).unwrap();

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
			0,
			Some(0),
			100,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			Asset::Eth,
			STABLE_ASSET,
			0,
			Some(tick),
			100_000,
		));
		assert_ok!(LiquidityPools::set_limit_order(
			RuntimeOrigin::signed(BOB),
			STABLE_ASSET,
			Asset::Eth,
			1,
			Some(tick),
			10_000,
		));
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &ALICE,),
			Some(PoolOrders {
				limit_orders: AssetsMap { base: vec![(0, 0, 100u128.into())], pair: vec![] },
				range_orders: vec![]
			})
		);

		let pallet_limit_orders =
			Pools::<Test>::get(asset_pair.canonical_asset_pair).unwrap().limit_orders_cache;
		assert_eq!(pallet_limit_orders.zero[&ALICE][&0], 0);
		assert_eq!(pallet_limit_orders.zero[&BOB][&0], tick);
		assert_eq!(pallet_limit_orders.one[&BOB][&1], tick);

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
		let pallet_limit_orders =
			Pools::<Test>::get(asset_pair.canonical_asset_pair).unwrap().limit_orders_cache;
		assert_eq!(pallet_limit_orders.zero.get(&ALICE), None);
		assert_eq!(pallet_limit_orders.zero.get(&BOB).unwrap().get(&0), Some(&100));
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
			Some(PoolInfo {
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
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &ALICE,),
			Some(PoolOrders {
				limit_orders: AssetsMap { base: vec![], pair: vec![] },
				range_orders: vec![(0, range.clone(), 1_000_000)]
			})
		);
		assert_eq!(
			LiquidityPools::pool_orders(Asset::Eth, STABLE_ASSET, &BOB,),
			Some(PoolOrders {
				limit_orders: AssetsMap { base: vec![], pair: vec![] },
				range_orders: vec![(0, range.clone(), 1_000_000)]
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
