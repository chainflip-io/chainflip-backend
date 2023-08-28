use core::panic;

use crate::{
	mock::*, utilities, CollectedNetworkFee, Error, FlipBuyInterval, FlipToBurn, LimitOrderQueue,
	OrderValidity, PoolQueryError, Pools, RangeOrderSize, STABLE_ASSET,
};
use cf_amm::{
	common::{sqrt_price_at_tick, SideMap, Tick},
	range_orders::AmountsToLiquidityError,
};
use cf_primitives::{chains::assets::any::Asset, AssetAmount};
use cf_test_utilities::assert_events_match;
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::Permill;

#[test]
fn can_create_new_trading_pool() {
	new_test_ext().execute_with(|| {
		let unstable_asset = Asset::Eth;
		let default_sqrt_price = sqrt_price_at_tick(0);

		// While the pool does not exist, no info can be obtained.
		assert!(Pools::<Test>::get(unstable_asset).is_none());

		// Fee must be appropriate
		assert_noop!(
			LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				unstable_asset,
				1_000_000u32,
				default_sqrt_price,
			),
			Error::<Test>::InvalidFeeAmount,
		);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			500_000u32,
			default_sqrt_price,
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::NewPoolCreated {
				unstable_asset,
				fee_hundredth_pips: 500_000u32,
				initial_sqrt_price: default_sqrt_price,
			},
		));

		// Cannot create duplicate pool
		assert_noop!(
			LiquidityPools::new_pool(
				RuntimeOrigin::root(),
				unstable_asset,
				0u32,
				default_sqrt_price
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
		let default_sqrt_price = sqrt_price_at_tick(0);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			500_000u32,
			default_sqrt_price,
		));

		// Disable the pool
		assert_ok!(LiquidityPools::update_pool_enabled(
			RuntimeOrigin::root(),
			unstable_asset,
			false
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PoolStateUpdated { unstable_asset, enabled: false },
		));

		assert_noop!(
			LiquidityPools::collect_and_mint_range_order(
				RuntimeOrigin::signed(ALICE),
				unstable_asset,
				range.clone(),
				RangeOrderSize::Liquidity(1_000_000),
			),
			Error::<Test>::PoolDisabled
		);

		// Re-enable the pool
		assert_ok!(LiquidityPools::update_pool_enabled(
			RuntimeOrigin::root(),
			unstable_asset,
			true
		));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::<Test>::PoolStateUpdated { unstable_asset, enabled: true },
		));

		assert_ok!(LiquidityPools::collect_and_mint_range_order(
			RuntimeOrigin::signed(ALICE),
			unstable_asset,
			range,
			RangeOrderSize::Liquidity(1_000_000),
		));
	});
}

#[test]
fn test_buy_back_flip_no_funds_available() {
	new_test_ext().execute_with(|| {
		let unstable_asset = Asset::Eth;
		let default_sqrt_price = sqrt_price_at_tick(0);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			500_000u32,
			default_sqrt_price,
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
			Default::default(),
			sqrt_price_at_tick(0),
		));
		assert_ok!(LiquidityPools::collect_and_mint_range_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			POSITION,
			RangeOrderSize::AssetAmounts {
				desired: SideMap::from_array([1_000_000, 1_000_000]),
				minimum: SideMap::from_array([900_000, 900_000]),
			},
		));
		let liquidity = assert_events_match!(
			Test,
			RuntimeEvent::LiquidityPools(
				crate::Event::RangeOrderMinted {
					liquidity,
					..
				},
			) => liquidity
		);
		assert_ok!(LiquidityPools::collect_and_burn_range_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			POSITION,
			liquidity
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
			Default::default(),
			sqrt_price_at_tick(0),
		));
		assert_ok!(LiquidityPools::collect_and_mint_range_order(
			RuntimeOrigin::signed(ALICE),
			FLIP,
			POSITION,
			RangeOrderSize::Liquidity(1_000_000),
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
fn can_get_liquidity_from_range_order() {
	new_test_ext().execute_with(|| {
		const POSITION: core::ops::Range<Tick> = -100_000..100_000;
		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Flip,
			Default::default(),
			sqrt_price_at_tick(0),
		));

		// Can get liquidity correctly.
		assert!(LiquidityPools::estimate_liquidity_from_range_order(
			Asset::Flip,
			POSITION.start,
			POSITION.end,
			1_000u128,
			1_000u128,
		)
		.is_ok());

		// Returns the correct error if pool does not exist
		assert_noop!(
			LiquidityPools::estimate_liquidity_from_range_order(
				Asset::Eth,
				POSITION.start,
				POSITION.end,
				1_000u128,
				1_000u128,
			),
			PoolQueryError::PoolDoesNotExist,
		);

		// Returns the correct error if pool does not exist
		assert_noop!(
			LiquidityPools::estimate_liquidity_from_range_order(
				Asset::Flip,
				POSITION.end,
				POSITION.start,
				1_000u128,
				1_000u128,
			),
			PoolQueryError::Inner(AmountsToLiquidityError::InvalidTickRange),
		);
	});
}

#[test]
fn can_mint_limit_order_with_validity() {
	new_test_ext().execute_with(|| {
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Flip,
			Default::default(),
			sqrt_price_at_tick(0),
		));
		assert_ok!(LiquidityPools::collect_and_mint_limit_order(
			RuntimeOrigin::signed(ALICE),
			Asset::Flip,
			crate::BuyOrSell::Buy,
			0,
			55,
			Some(OrderValidity::new(Some(2..5), Some(6))),
		));
		// Not yet minted.
		assert!(!LimitOrderQueue::<Test>::get(2).is_empty());
		// We mint the order.
		LiquidityPools::on_initialize(2);
		// Removed as minted from the order queue.
		assert!(
			LimitOrderQueue::<Test>::get(2).is_empty(),
			"Should be empty, but is {:?}",
			LimitOrderQueue::<Test>::get(2)
		);
		// Order is getting moved to the block where we expect it to ge burned.
		assert!(!LimitOrderQueue::<Test>::get(6).is_empty());
	});
}

#[test]
fn gets_rejected_if_order_window_has_already_passed() {
	new_test_ext().execute_with(|| {
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			Asset::Flip,
			Default::default(),
			sqrt_price_at_tick(0),
		));
		System::set_block_number(1);
		assert_noop!(
			LiquidityPools::collect_and_mint_limit_order(
				RuntimeOrigin::signed(ALICE),
				Asset::Flip,
				crate::BuyOrSell::Buy,
				0,
				55,
				Some(OrderValidity::new(Some(0..1), None)),
			),
			Error::<Test>::OrderValidityExpired
		);
	});
}

mod order_validity_tests {
	use super::*;

	#[test]
	fn default_is_always_valid() {
		let validity = OrderValidity::default();

		assert!(validity.is_valid_at(0));
		assert!(validity.is_valid_at(1));
		assert!(validity.is_valid_at(BlockNumberFor::<Test>::MAX));

		assert!(validity.is_ready_at(0));
		assert!(validity.is_ready_at(1));
		assert!(validity.is_ready_at(BlockNumberFor::<Test>::MAX));
	}

	#[test]
	fn validity_window() {
		let validity = OrderValidity::new(Some(2u32..5), None);

		assert!(validity.is_valid_at(1));
		assert!(validity.is_valid_at(2));
		assert!(validity.is_valid_at(3));
		assert!(validity.is_valid_at(4));
		assert!(!validity.is_valid_at(5));

		assert!(!validity.is_ready_at(1));
		assert!(validity.is_ready_at(2));
		assert!(validity.is_ready_at(3));
		assert!(validity.is_ready_at(4));
		assert!(!validity.is_ready_at(5));
	}

	#[test]
	fn expiry() {
		let validity = OrderValidity::new(None, Some(3));

		assert!(validity.is_valid_at(2));
		assert!(!validity.is_valid_at(3));
		assert!(!validity.is_valid_at(4));

		assert!(validity.is_ready_at(2));
		assert!(!validity.is_ready_at(3));
		assert!(!validity.is_ready_at(4));
	}

	#[test]
	fn combined() {
		let validity = OrderValidity::new(Some(2..4), Some(5));

		assert!(validity.is_valid_at(2));
		assert!(validity.is_valid_at(3));
		assert!(!validity.is_valid_at(4));
		assert!(!validity.is_valid_at(5));
		assert!(!validity.is_valid_at(6));

		assert!(validity.is_ready_at(2));
		assert!(validity.is_ready_at(3));
		assert!(!validity.is_ready_at(4));
		assert!(!validity.is_ready_at(5));
		assert!(!validity.is_ready_at(5));
	}
}

/*
#[test]
fn can_update_liquidity_fee() {
	new_test_ext().execute_with(|| {
		let range = -100..100;
		let unstable_asset = Asset::Eth;
		let default_sqrt_price = sqrt_price_at_tick(0);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			500_000u32,
			default_sqrt_price,
		));
		assert_ok!(LiquidityPools::collect_and_mint_range_order(
			RuntimeOrigin::signed(ALICE),
			unstable_asset,
			range,
			1_000_000,
		));

		assert_ok!(LiquidityPools::swap(unstable_asset, Asset::Usdc, 1_000));

		// Current swap fee is 50%
		System::assert_has_event(RuntimeEvent::LiquidityPools(crate::Event::AssetSwapped {
			from: Asset::Flip,
			to: Asset::Usdc,
			input_amount: 1000,
			output_amount: 499,
		}));

		// Fee must be within the allowable range.
		assert_noop!(
			LiquidityPools::set_liquidity_fee(RuntimeOrigin::root(), unstable_asset, 500001u32),
			Error::<Test>::InvalidFeeAmount
		);

		// Set the fee to 0%
		assert_ok!(LiquidityPools::set_liquidity_fee(RuntimeOrigin::root(), unstable_asset, 0u32));
		System::assert_last_event(RuntimeEvent::LiquidityPools(
			crate::Event::LiquidityFeeUpdated {
				unstable_asset: Asset::Flip,
				fee_hundredth_pips: 0u32,
			},
		));

		System::reset_events();
		assert_ok!(LiquidityPools::swap(unstable_asset, Asset::Usdc, 1_000));

		// Current swap fee is now 0%
		System::assert_has_event(RuntimeEvent::LiquidityPools(crate::Event::AssetSwapped {
			from: Asset::Flip,
			to: Asset::Usdc,
			input_amount: 1000,
			output_amount: 998,
		}));
	});
}

#[test]
fn can_get_liquidity_and_positions() {
	new_test_ext().execute_with(|| {
		let range_1 = -100..100;
		let range_2 = -50..200;
		let unstable_asset = Asset::Flip;
		let default_sqrt_price = sqrt_price_at_tick(0);

		// Create a new pool.
		assert_ok!(LiquidityPools::new_pool(
			RuntimeOrigin::root(),
			unstable_asset,
			500_000u32,
			default_sqrt_price,
		));

		assert_ok!(LiquidityPools::collect_and_mint_range_order(
			RuntimeOrigin::signed(ALICE),
			unstable_asset,
			range_1,
			1_000,
		));
		assert_ok!(LiquidityPools::collect_and_mint_range_order(
			RuntimeOrigin::signed(ALICE),
			unstable_asset,
			range_2,
			2_000,
		));

		assert_eq!(
			LiquidityPools::minted_positions(&ALICE, &unstable_asset),
			vec![(range_1.lower, range_1.upper, 1_000), (range_2.lower, range_2.upper, 2_000),]
		);
		assert_eq!(LiquidityPools::minted_positions(&[1u8; 32].into(), &unstable_asset), vec![]);
	});
}
*/
