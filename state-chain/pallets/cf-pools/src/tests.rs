use crate::{mock::*, Error, Pools};
use cf_primitives::{chains::assets::any::Asset, AmmRange, AssetAmount, PoolAssetMap};
use cf_traits::LiquidityPoolApi;
use frame_support::{assert_noop, assert_ok};

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
				|_: PoolAssetMap<AssetAmount>| true,
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
			|_: PoolAssetMap<AssetAmount>| true
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
			|_: PoolAssetMap<AssetAmount>| true
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
				|_: PoolAssetMap<AssetAmount>| true
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
			|_: PoolAssetMap<AssetAmount>| true
		));
	});
}
