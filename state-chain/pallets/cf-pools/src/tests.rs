use crate::{mock::*, CollectedNetworkFee, Error, FlipBuyInterval, Pools};
use cf_primitives::{chains::assets::any::Asset, AmmRange, AssetAmount, PoolAssetMap};
use cf_traits::LiquidityPoolApi;
use frame_support::{assert_noop, assert_ok};

use crate::FlipToBurn;
use frame_support::traits::Hooks;

#[test]
fn can_create_new_trading_pool() {
	new_test_ext().execute_with(|| {
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;
		let default_tick_price = 0;
		// Pool does not exist.
		assert!(Pools::<Test>::get(asset).is_none());
		assert_noop!(
			LiquidityPools::mint(
				LP.into(),
				asset,
				range,
				1_000_000,
				|_: PoolAssetMap<AssetAmount>| Ok(()),
			),
			Error::<Test>::PoolDoesNotExist,
		);
		assert_eq!(LiquidityPools::current_tick(&asset), None);

		// Fee must be between 0 - 50%
		assert_noop!(
			LiquidityPools::new_pool(RuntimeOrigin::root(), asset, 500_001u32, default_tick_price,),
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
		assert_ok!(LiquidityPools::mint(
			LP.into(),
			asset,
			range,
			1_000_000,
			|_: PoolAssetMap<AssetAmount>| Ok(())
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
		assert_ok!(LiquidityPools::mint(
			LP.into(),
			asset,
			range,
			1_000_000,
			|_: PoolAssetMap<AssetAmount>| Ok(())
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

		FlipBuyInterval::<Test>::set(5);
		CollectedNetworkFee::<Test>::set(30);
		LiquidityPools::on_initialize(8);
		assert_eq!(FlipToBurn::<Test>::get(), 0);
	});
}

#[test]
fn test_buy_back_flip() {
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

		FlipBuyInterval::<Test>::set(5);
		CollectedNetworkFee::<Test>::set(30);
		LiquidityPools::on_initialize(10);
		let initial_flip_to_burn = FlipToBurn::<Test>::get();
		// Expect the some funds available to burn
		assert!(initial_flip_to_burn != 0);
		CollectedNetworkFee::<Test>::set(30);
		LiquidityPools::on_initialize(14);
		// Expect nothing to change because we didn't passed the buy interval threshold
		assert_eq!(initial_flip_to_burn, FlipToBurn::<Test>::get());
		LiquidityPools::on_initialize(15);
		// Expect the amount of Flip we can burn to increase
		assert!(initial_flip_to_burn < FlipToBurn::<Test>::get(), "flip to burn didn't increased!");
	});
}

#[test]
fn test_network_fee_calculation() {
	new_test_ext().execute_with(|| {
		// Show we can never overflow and panic
		LiquidityPools::calc_fee(u16::MAX, AssetAmount::MAX);
		// 200 bps (2%) of 100 = 2
		assert_eq!(LiquidityPools::calc_fee(200, 100), 2);
		// 2220 bps = 22 % of 199 = 43,78
		assert_eq!(LiquidityPools::calc_fee(2220, 199), 44);
		// 2220 bps = 22 % of 234 = 51,26
		assert_eq!(LiquidityPools::calc_fee(2220, 233), 52);
		// 10 bps = 0,1% of 3000 = 3
		assert_eq!(LiquidityPools::calc_fee(10, 3000), 3);
	});
}
