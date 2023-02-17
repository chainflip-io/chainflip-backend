use crate::{
	mock::*, CollectedNetworkFee, Error, FlipBuyInterval, FlipToBurn, Pools, STABLE_ASSET,
};
use cf_primitives::{chains::assets::any::Asset, AmmRange, AssetAmount, PoolAssetMap};
use cf_traits::{LiquidityPoolApi, SwappingApi};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_runtime::Permill;

#[test]
fn can_create_new_trading_pool() {
	new_test_ext().execute_with(|| {
		let asset = Asset::Eth;
		let default_tick_price = 0;

		// While the pool does not exist, no info can be obtained.
		assert!(Pools::<Test>::get(asset).is_none());
		assert_eq!(LiquidityPools::current_tick(&asset), None);

		// Fee must be appropriate
		assert_noop!(
			LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				asset,
				1_000_000u32,
				default_tick_price,
			),
			Error::<Test>::InvalidFeeAmount,
		);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			asset,
			500_000u32,
			default_tick_price,
		));
		assert_eq!(LiquidityPools::current_tick(&asset), Some(0));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::NewPoolCreated {
				asset,
				fee_100th_bips: 500_000u32,
				initial_tick_price: default_tick_price,
			},
		));

		// Cannot create duplicate pool
		assert_noop!(
			LiquidityPools::new_pool(RuntimeOrigin::root(), asset, 0u32, default_tick_price,),
			Error::<Test>::PoolAlreadyExists
		);
	});
}

#[test]
fn can_enable_disable_trading_pool() {
	new_test_ext().execute_with(|| {
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;
		let default_tick_price = 0;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			asset,
			500_000u32,
			default_tick_price,
		));

		// Disable the pool
		assert_ok!(LiquidityPools::update_pool_enabled(RuntimeOrigin::root(), asset, false));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PoolStateUpdated { asset, enabled: false },
		));

		assert_noop!(
			LiquidityPools::mint(
				LP.into(),
				asset,
				range,
				1_000_000,
				|_: PoolAssetMap<AssetAmount>| Ok(())
			),
			Error::<Test>::PoolDisabled
		);

		// Re-enable the pool
		assert_ok!(LiquidityPools::update_pool_enabled(RuntimeOrigin::root(), asset, true));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PoolStateUpdated { asset, enabled: true },
		));

		assert_ok!(LiquidityPools::mint(
			LP.into(),
			asset,
			range,
			1_000_000,
			|_: PoolAssetMap<AssetAmount>| Ok(())
		));
	});
}

#[test]
fn test_buy_back_flip_no_funds_available() {
	new_test_ext().execute_with(|| {
		let asset = Asset::Flip;
		let default_tick_price = 0;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			asset,
			500_000u32,
			default_tick_price,
		));

		FlipBuyInterval::<Test>::set(5);
		CollectedNetworkFee::<Test>::set(30);
		LiquidityPools::on_initialize(8);
		assert_eq!(FlipToBurn::<Test>::get(), 0);
	});
}

#[test]
fn test_buy_back_flip() {
	new_test_ext().execute_with(|| {
		const COLLECTED_FEE: AssetAmount = 30;
		const INTERVAL: <Test as frame_system::Config>::BlockNumber = 5;
		const POSITION: AmmRange = AmmRange { lower: -100_000, upper: 100_000 };
		const FLIP: Asset = Asset::Flip;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			FLIP,
			Default::default(),
			Default::default(),
		));
		assert_ok!(LiquidityPools::mint(
			LP.into(),
			FLIP,
			POSITION,
			1_000_000,
			|_: PoolAssetMap<AssetAmount>| Ok(()),
		));

		// Swapping should cause the network fee to be collected.
		LiquidityPools::swap(FLIP, STABLE_ASSET, 1000).unwrap();
		LiquidityPools::swap(STABLE_ASSET, FLIP, 1000).unwrap();

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
		LiquidityPools::calc_fee(Permill::from_percent(100), AssetAmount::MAX);
		// 200 bps (2%) of 100 = 2
		assert_eq!(LiquidityPools::calc_fee(Permill::from_percent(2u32), 100), 2);
		// 2220 bps = 22 % of 199 = 43,78
		assert_eq!(LiquidityPools::calc_fee(Permill::from_rational(2220u32, 10000u32), 199), 44);
		// 2220 bps = 22 % of 234 = 51,26
		assert_eq!(LiquidityPools::calc_fee(Permill::from_rational(2220u32, 10000u32), 233), 52);
		// 10 bps = 0,1% of 3000 = 3
		assert_eq!(LiquidityPools::calc_fee(Permill::from_rational(1u32, 1000u32), 3000), 3);
	});
}

#[test]
fn can_update_liquidity_fee() {
	new_test_ext().execute_with(|| {
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Flip;
		let default_tick_price = 0;

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			asset,
			500_000u32,
			default_tick_price,
		));
		assert_ok!(LiquidityPools::mint(
			LP.into(),
			asset,
			range,
			1_000_000,
			|_: PoolAssetMap<AssetAmount>| Ok(())
		));

		assert_ok!(LiquidityPools::swap(asset, Asset::Usdc, 1_000));

		// Current swap fee is 50%
		System::assert_has_event(RuntimeEvent::LiquidityPools(crate::Event::AssetsSwapped {
			from: Asset::Flip,
			to: Asset::Usdc,
			input: 1000,
			output: 499,
			liquidity_fee: 500,
		}));

		// Fee must be within the allowable range.
		assert_noop!(
			LiquidityPools::set_liquidity_fee(RuntimeOrigin::root(), asset, 500001u32),
			Error::<Test>::InvalidFeeAmount
		);

		// Set the fee to 0%
		assert_ok!(LiquidityPools::set_liquidity_fee(RuntimeOrigin::root(), asset, 0u32));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::LiquidityFeeUpdated { asset: Asset::Flip, fee_100th_bips: 0u32 },
		));

		System::reset_events();
		assert_ok!(LiquidityPools::swap(asset, Asset::Usdc, 1_000));

		// Current swap fee is now 0%
		System::assert_has_event(RuntimeEvent::LiquidityPools(crate::Event::AssetsSwapped {
			from: Asset::Flip,
			to: Asset::Usdc,
			input: 1000,
			output: 998,
			liquidity_fee: 0,
		}));
	});
}
