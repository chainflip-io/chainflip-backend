use crate::{mock::*, FreeBalances};

use cf_primitives::{
	liquidity::AmmRange, Asset, ExchangeRate, ForeignChainAddress, TradingPosition,
};
use cf_traits::{mocks::system_state_info::MockSystemStateInfo, LiquidityPoolApi, SystemStateInfo};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};

#[test]
fn only_liquidity_provider_can_manage_positions() {
	new_test_ext().execute_with(|| {
		let position = TradingPosition::ClassicV3 {
			range: AmmRange { lower: 0, upper: 0 },
			volume_0: 100,
			volume_1: 1000,
		};
		let asset = Asset::Eth;

		assert_noop!(
			LiquidityProvider::open_position(
				RuntimeOrigin::signed(NON_LP_ACCOUNT),
				asset,
				position,
			),
			BadOrigin,
		);

		assert_noop!(
			LiquidityProvider::update_position(
				RuntimeOrigin::signed(NON_LP_ACCOUNT),
				asset,
				0,
				position,
			),
			BadOrigin,
		);

		assert_noop!(
			LiquidityProvider::close_position(RuntimeOrigin::signed(NON_LP_ACCOUNT), 0),
			BadOrigin,
		);
	});
}

#[test]
fn egress_chain_and_asset_must_match() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				RuntimeOrigin::signed(LP_ACCOUNT),
				1,
				Asset::Eth,
				ForeignChainAddress::Dot([0x00; 32]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				RuntimeOrigin::signed(LP_ACCOUNT),
				1,
				Asset::Dot,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
	});
}

#[test]
fn liquidity_providers_can_withdraw_liquidity() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(LP_ACCOUNT, Asset::Eth, 1_000);

		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				RuntimeOrigin::signed(LP_ACCOUNT),
				100,
				Asset::Dot,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);

		assert_ok!(LiquidityProvider::withdraw_liquidity(
			RuntimeOrigin::signed(LP_ACCOUNT),
			100,
			Asset::Eth,
			ForeignChainAddress::Eth([0x00; 20]),
		));

		assert_eq!(FreeBalances::<Test>::get(LP_ACCOUNT, Asset::Eth), Some(900));
	});
}

#[test]
fn cannot_deposit_and_withdrawal_during_maintenance() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(LP_ACCOUNT, Asset::Eth, 1_000);

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		// Cannot request deposit address during maintenance.
		assert_noop!(
			LiquidityProvider::request_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT),
				Asset::Eth,
			),
			"We are in maintenance!"
		);

		// Cannot withdraw liquidity during maintenance.
		assert_noop!(
			LiquidityProvider::withdraw_liquidity(
				RuntimeOrigin::signed(LP_ACCOUNT),
				100,
				Asset::Eth,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			"We are in maintenance!"
		);

		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		assert!(!MockSystemStateInfo::is_maintenance_mode());

		// Deposit and withdrawal can now work as per normal.
		assert_ok!(LiquidityProvider::request_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			Asset::Eth,
		));

		assert_ok!(LiquidityProvider::withdraw_liquidity(
			RuntimeOrigin::signed(LP_ACCOUNT),
			100,
			Asset::Eth,
			ForeignChainAddress::Eth([0x00; 20]),
		));
	});
}

#[test]
fn cannot_manage_liquidity_during_maintenance() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(LP_ACCOUNT, Asset::Eth, 1_000_000);
		FreeBalances::<Test>::insert(LP_ACCOUNT, Asset::Usdc, 1_000_000);

		let position = TradingPosition::ClassicV3 {
			range: AmmRange { lower: 0, upper: 0 },
			volume_0: 100,
			volume_1: 1000,
		};
		let asset = Asset::Eth;

		LiquidityPools::deploy(
			&asset,
			TradingPosition::ClassicV3 {
				range: Default::default(),
				volume_0: 1_000_000,
				volume_1: 5_000_000,
			},
		);

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		assert_noop!(
			LiquidityProvider::open_position(RuntimeOrigin::signed(LP_ACCOUNT), asset, position,),
			"We are in maintenance!"
		);
		assert_noop!(
			LiquidityProvider::update_position(
				RuntimeOrigin::signed(LP_ACCOUNT),
				asset,
				0,
				position,
			),
			"We are in maintenance!"
		);
		assert_noop!(
			LiquidityProvider::close_position(RuntimeOrigin::signed(LP_ACCOUNT), 0,),
			"We are in maintenance!"
		);

		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		assert!(!MockSystemStateInfo::is_maintenance_mode());

		assert_ok!(LiquidityProvider::open_position(
			RuntimeOrigin::signed(LP_ACCOUNT),
			asset,
			position
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT),
			asset,
			0,
			position,
		));
		assert_ok!(LiquidityProvider::close_position(RuntimeOrigin::signed(LP_ACCOUNT), 0,),);
	});
}

#[test]
fn can_open_and_close_liquidity() {
	new_test_ext().execute_with(|| {
		const ASSET: Asset = Asset::Flip;
		LiquidityPools::deploy(
			&ASSET,
			TradingPosition::ClassicV3 {
				range: Default::default(),
				volume_0: 1_000_000,
				volume_1: 5_000_000,
			},
		);
		assert_eq!(
			LiquidityPools::swap_rate(ASSET, Asset::Usdc, 0),
			ExchangeRate::from_rational(5_000_000, 1_000_000)
		);

		FreeBalances::<Test>::insert(LP_ACCOUNT, ASSET, 1_000_000);
		FreeBalances::<Test>::insert(LP_ACCOUNT, Asset::Usdc, 1_000_000);

		// Can open position
		let position = TradingPosition::ClassicV3 {
			range: Default::default(),
			volume_0: 1_000,
			volume_1: 2_000,
		};
		assert_ok!(LiquidityProvider::open_position(
			RuntimeOrigin::signed(LP_ACCOUNT),
			ASSET,
			position,
		));

		assert_eq!(
			LiquidityPools::swap_rate(ASSET, Asset::Usdc, 0),
			ExchangeRate::from_rational(5_002_000, 1_001_000)
		);

		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			crate::Event::<Test>::TradingPositionOpened {
				account_id: LP_ACCOUNT,
				position_id: 0,
				asset: ASSET,
				position,
			},
		));

		// Test close position
		assert_ok!(LiquidityProvider::close_position(RuntimeOrigin::signed(LP_ACCOUNT), 0,));

		assert_eq!(FreeBalances::<Test>::get(LP_ACCOUNT, ASSET), Some(1_000_000));
		assert_eq!(FreeBalances::<Test>::get(LP_ACCOUNT, Asset::Usdc), Some(1_000_000));
		assert_eq!(
			LiquidityPools::swap_rate(ASSET, Asset::Usdc, 0),
			ExchangeRate::from_rational(5_000_000, 1_000_000)
		);
	});
}
