use crate::{mock::*, FreeBalances};

use cf_chains::address::ForeignChainAddress;
use cf_primitives::{liquidity::AmmRange, AccountId, Asset, PoolAssetMap};
use cf_traits::{mocks::system_state_info::MockSystemStateInfo, LiquidityPoolApi, SystemStateInfo};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};

fn provision_accounts() {
	FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000_000);
	FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Usdc, 1_000_000);
}

#[test]
fn only_liquidity_provider_can_manage_positions() {
	new_test_ext().execute_with(|| {
		provision_accounts();
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_noop!(
			LiquidityProvider::update_position(
				RuntimeOrigin::signed(NON_LP_ACCOUNT.into()),
				asset,
				range,
				1_000_000,
			),
			BadOrigin,
		);
	});
}

#[test]
fn egress_chain_and_asset_must_match() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				1,
				Asset::Eth,
				ForeignChainAddress::Dot([0x00; 32]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
	});
}

#[test]
fn liquidity_providers_can_withdraw_asset() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000);
		FreeBalances::<Test>::insert(AccountId::from(NON_LP_ACCOUNT), Asset::Eth, 1_000);

		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				100,
				Asset::Dot,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);

		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(NON_LP_ACCOUNT.into()),
				100,
				Asset::Eth,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			BadOrigin
		);

		assert_ok!(LiquidityProvider::withdraw_asset(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			100,
			Asset::Eth,
			ForeignChainAddress::Eth([0x00; 20]),
		));

		assert_eq!(FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth), Some(900));
	});
}

#[test]
fn cannot_deposit_and_withdrawal_during_maintenance() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000);

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		// Cannot request deposit address during maintenance.
		assert_noop!(
			LiquidityProvider::request_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				Asset::Eth,
			),
			"We are in maintenance!"
		);

		// Cannot withdraw liquidity during maintenance.
		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
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
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
		));

		assert_ok!(LiquidityProvider::withdraw_asset(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			100,
			Asset::Eth,
			ForeignChainAddress::Eth([0x00; 20]),
		));
	});
}

#[test]
fn cannot_manage_positions_during_maintenance() {
	new_test_ext().execute_with(|| {
		provision_accounts();
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(RuntimeOrigin::root(), asset, 0, 0,));

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		assert_noop!(
			LiquidityProvider::update_position(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				asset,
				range,
				1_000_000,
			),
			"We are in maintenance!",
		);

		// Deactivate maintenance mode
		MockSystemStateInfo::set_maintenance(false);
		assert!(!MockSystemStateInfo::is_maintenance_mode());

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			1_000_000,
		));
	});
}

#[test]
fn can_mint_liquidity() {
	new_test_ext().execute_with(|| {
		provision_accounts();
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(RuntimeOrigin::root(), asset, 0, 0,));
		System::reset_events();

		// Can open a new position
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			1_000_000,
		));

		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(995_012)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(995_012)
		);

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityMinted {
				lp: LP_ACCOUNT.into(),
				asset,
				range,
				minted_liquidity: 1_000_000,
				assets_debited: PoolAssetMap::new(4988, 4988),
				fees_harvested: Default::default(),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_debited: 4988,
		}));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_debited: 4988,
		}));

		assert_eq!(LiquidityPools::minted_liquidity(&LP_ACCOUNT.into(), &asset, range), 1_000_000);

		// Can mint more liquidity (+1000)
		System::reset_events();
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			1_001_000,
		));

		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(995_007)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(995_007)
		);

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityMinted {
				lp: LP_ACCOUNT.into(),
				asset,
				range,
				minted_liquidity: 1_000,
				assets_debited: PoolAssetMap::new(5, 5),
				fees_harvested: Default::default(),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_debited: 5,
		}));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_debited: 5,
		}));

		assert_eq!(LiquidityPools::minted_liquidity(&LP_ACCOUNT.into(), &asset, range), 1_001_000,);
	});
}

#[test]
fn can_burn_liquidity() {
	new_test_ext().execute_with(|| {
		provision_accounts();
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(RuntimeOrigin::root(), asset, 0, 0,));

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			1_000_000,
		));

		// Can partially burn a liquidity position (-500_000)
		System::reset_events();
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			500_000,
		));

		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(997_505)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(997_505)
		);

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: LP_ACCOUNT.into(),
				asset,
				range,
				burnt_liquidity: 500_000,
				assets_returned: PoolAssetMap::new(2493, 2493),
				fees_harvested: Default::default(),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_credited: 2493,
		}));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_credited: 2493,
		}));

		assert_eq!(LiquidityPools::minted_liquidity(&LP_ACCOUNT.into(), &asset, range), 500_000,);

		// Can fully burn a position
		System::reset_events();
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			0,
		));

		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(999_998)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(999_998)
		);

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: LP_ACCOUNT.into(),
				asset,
				range,
				burnt_liquidity: 500_000,
				assets_returned: PoolAssetMap::new(2_493, 2_493),
				fees_harvested: Default::default(),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_credited: 2_493,
		}));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_credited: 2_493,
		}));

		assert_eq!(LiquidityPools::minted_liquidity(&LP_ACCOUNT.into(), &asset, range), 0);
	});
}

#[test]
fn mint_fails_with_insufficient_balance() {
	new_test_ext().execute_with(|| {
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(RuntimeOrigin::root(), asset, 0, 0,));
		System::reset_events();

		// Cannot open a new position
		assert_noop!(
			LiquidityProvider::update_position(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				asset,
				range,
				1_000_000,
			),
			crate::Error::<Test>::InsufficientBalance
		);
	});
}
