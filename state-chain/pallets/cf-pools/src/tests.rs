use crate::{
	mock::*, CollectedNetworkFee, Error, FlipBuyInterval, FlipToBurn, Pools, SwapResult,
	STABLE_ASSET,
};
use cf_amm::common::{sqrt_price_at_tick, Tick};
use cf_primitives::{chains::assets::any::Asset, AssetAmount, ExchangeRate};
use cf_traits::SwappingApi;
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_runtime::{FixedPointNumber, Permill};

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
				1_000_000,
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
			1_000_000,
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
fn test_buy_back_flip() {
	new_test_ext().execute_with(|| {
		const COLLECTED_FEE: AssetAmount = 30;
		const INTERVAL: <Test as frame_system::Config>::BlockNumber = 5;
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
			1_000_000,
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
		LiquidityPools::calculate_network_fee(Permill::from_percent(100), AssetAmount::MAX);
		// 200 bps (2%) of 100 = 2
		assert_eq!(
			LiquidityPools::calculate_network_fee(Permill::from_percent(2u32), 100),
			(98, 2)
		);
		// 2220 bps = 22 % of 199 = 43,78
		assert_eq!(
			LiquidityPools::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), 199),
			(155, 44)
		);
		// 2220 bps = 22 % of 234 = 51,26
		assert_eq!(
			LiquidityPools::calculate_network_fee(Permill::from_rational(2220u32, 10000u32), 233),
			(181, 52)
		);
		// 10 bps = 0,1% of 3000 = 3
		assert_eq!(
			LiquidityPools::calculate_network_fee(Permill::from_rational(1u32, 1000u32), 3000),
			(2997, 3)
		);
	});
}

fn setup_eth_and_dot_pool() {
	// Setup exchange pool with non-default exchange rate
	// Setup Eth pool.
	assert_ok!(LiquidityPools::new_pool(
		RuntimeOrigin::root(),
		Asset::Eth,
		0u32,
		sqrt_price_at_tick(1_000),
	));
	assert_ok!(LiquidityPools::collect_and_mint_range_order(
		RuntimeOrigin::signed(ALICE),
		Asset::Eth,
		900..1100,
		1_000_000,
	));

	// Setup Dot pool.
	assert_ok!(LiquidityPools::new_pool(
		RuntimeOrigin::root(),
		Asset::Dot,
		0u32,
		sqrt_price_at_tick(-2_500),
	));
	assert_ok!(LiquidityPools::collect_and_mint_range_order(
		RuntimeOrigin::signed(ALICE),
		Asset::Dot,
		-2_600..-2_400,
		1_000_000,
	));
}

#[test]
fn can_get_swap_rate_into_stable() {
	new_test_ext().execute_with(|| {
		setup_eth_and_dot_pool();
		let amount = 1_000;

		// Can get swap rate for Eth -> STABLE
		let expected_rate =
			LiquidityPools::swap_rate_exchange_rate(Asset::Eth, Asset::Usdc, amount).unwrap();
		let expected_output =
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Usdc, amount).unwrap();
		let actual_output = LiquidityPools::swap(Asset::Eth, Asset::Usdc, amount).unwrap();

		assert_eq!(expected_rate, ExchangeRate::saturating_from_rational(actual_output, amount));
		assert_eq!(expected_output, SwapResult::IntoStable(actual_output));
	});
}

#[test]
fn can_get_swap_rate_from_stable() {
	new_test_ext().execute_with(|| {
		setup_eth_and_dot_pool();
		let amount = 1_000;

		// Can get swap rate for STABLE -> ETH
		let expected_rate =
			LiquidityPools::swap_rate_exchange_rate(Asset::Usdc, Asset::Eth, amount).unwrap();
		let expected_output =
			LiquidityPools::swap_rate_output_amount(Asset::Usdc, Asset::Eth, amount).unwrap();
		let actual_output = LiquidityPools::swap(Asset::Usdc, Asset::Eth, amount).unwrap();

		assert_eq!(expected_rate, ExchangeRate::saturating_from_rational(actual_output, amount));
		assert_eq!(expected_output, SwapResult::FromStable(actual_output));
	});
}

#[test]
fn can_get_swap_rate_through_stable() {
	new_test_ext().execute_with(|| {
		setup_eth_and_dot_pool();
		let amount = 1_000;

		// Can get swap rate for STABLE -> ETH
		let expected_rate =
			LiquidityPools::swap_rate_exchange_rate(Asset::Eth, Asset::Dot, amount).unwrap();
		let expected_output =
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Dot, amount).unwrap();
		let expected_intermediate_amount =
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Usdc, amount)
				.unwrap()
				.output_amount();
		let actual_output = LiquidityPools::swap(Asset::Eth, Asset::Dot, amount).unwrap();

		assert_eq!(expected_rate, ExchangeRate::saturating_from_rational(actual_output, amount));
		assert_eq!(
			expected_output,
			SwapResult::ThroughStable(expected_intermediate_amount, actual_output)
		);
	});
}

#[test]
fn getting_swap_rate_does_not_change_storage() {
	new_test_ext().execute_with(|| {
		setup_eth_and_dot_pool();
		let amount = 1_000;

		// Getting exchange rate repeatedly does not change the exchange rate
		assert_eq!(
			LiquidityPools::swap_rate_exchange_rate(Asset::Eth, Asset::Dot, amount).unwrap(),
			LiquidityPools::swap_rate_exchange_rate(Asset::Eth, Asset::Dot, amount).unwrap()
		);
		assert_eq!(
			LiquidityPools::swap_rate_exchange_rate(Asset::Eth, Asset::Usdc, amount).unwrap(),
			LiquidityPools::swap_rate_exchange_rate(Asset::Eth, Asset::Usdc, amount).unwrap()
		);
		assert_eq!(
			LiquidityPools::swap_rate_exchange_rate(Asset::Usdc, Asset::Eth, amount).unwrap(),
			LiquidityPools::swap_rate_exchange_rate(Asset::Usdc, Asset::Eth, amount).unwrap()
		);

		// Getting output amount repeatedly does not change the exchange rate
		assert_eq!(
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Dot, amount).unwrap(),
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Dot, amount).unwrap()
		);
		assert_eq!(
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Usdc, amount).unwrap(),
			LiquidityPools::swap_rate_output_amount(Asset::Eth, Asset::Usdc, amount).unwrap()
		);
		assert_eq!(
			LiquidityPools::swap_rate_output_amount(Asset::Usdc, Asset::Eth, amount).unwrap(),
			LiquidityPools::swap_rate_output_amount(Asset::Usdc, Asset::Eth, amount).unwrap()
		);

		// Only swap changes the swap rate
		assert_ne!(
			LiquidityPools::swap(Asset::Eth, Asset::Dot, amount).unwrap(),
			LiquidityPools::swap(Asset::Eth, Asset::Dot, amount).unwrap()
		);
		assert_ne!(
			LiquidityPools::swap(Asset::Eth, Asset::Usdc, amount).unwrap(),
			LiquidityPools::swap(Asset::Eth, Asset::Usdc, amount).unwrap()
		);
		assert_ne!(
			LiquidityPools::swap(Asset::Usdc, Asset::Eth, amount).unwrap(),
			LiquidityPools::swap(Asset::Usdc, Asset::Eth, amount).unwrap()
		);
	});
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
