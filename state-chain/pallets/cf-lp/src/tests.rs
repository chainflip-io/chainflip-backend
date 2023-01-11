use crate::{mock::*, FreeBalances};

use cf_amm::PoolState;
use cf_primitives::{
	liquidity::AmmRange, AccountId, Asset, ForeignChainAddress, MintedLiquidity, PoolAssetMap,
};
use cf_traits::{
	mocks::system_state_info::MockSystemStateInfo, LiquidityPoolApi, SwappingApi, SystemStateInfo,
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use sp_core::U256;

#[test]
fn only_liquidity_provider_can_manage_positions() {
	new_test_ext().execute_with(|| {
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_noop!(
			LiquidityProvider::update_position(
				Origin::signed(NON_LP_ACCOUNT.into()),
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
			LiquidityProvider::withdraw_free_balances(
				Origin::signed(LP_ACCOUNT.into()),
				1,
				Asset::Eth,
				ForeignChainAddress::Dot([0x00; 32]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
		assert_noop!(
			LiquidityProvider::withdraw_free_balances(
				Origin::signed(LP_ACCOUNT.into()),
				1,
				Asset::Dot,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);
	});
}

#[test]
fn liquidity_providers_can_withdraw_free_balances() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000);

		assert_noop!(
			LiquidityProvider::withdraw_free_balances(
				Origin::signed(LP_ACCOUNT.into()),
				100,
				Asset::Dot,
				ForeignChainAddress::Eth([0x00; 20]),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);

		assert_ok!(LiquidityProvider::withdraw_free_balances(
			Origin::signed(LP_ACCOUNT.into()),
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
				Origin::signed(LP_ACCOUNT.into()),
				Asset::Eth,
			),
			"We are in maintenance!"
		);

		// Cannot withdraw liquidity during maintenance.
		assert_noop!(
			LiquidityProvider::withdraw_free_balances(
				Origin::signed(LP_ACCOUNT.into()),
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
			Origin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
		));

		assert_ok!(LiquidityProvider::withdraw_free_balances(
			Origin::signed(LP_ACCOUNT.into()),
			100,
			Asset::Eth,
			ForeignChainAddress::Eth([0x00; 20]),
		));
	});
}

#[test]
fn cannot_manage_liquidity_during_maintenance() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000_000);
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Usdc, 1_000_000);

		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(
			Origin::root(),
			asset,
			0,
			PoolState::sqrt_price_at_tick(0),
		));

		// Activate maintenance mode
		MockSystemStateInfo::set_maintenance(true);
		assert!(MockSystemStateInfo::is_maintenance_mode());

		assert_noop!(
			LiquidityProvider::update_position(
				Origin::signed(LP_ACCOUNT.into()),
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
			Origin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			1_000_000,
		));
	});
}

#[test]
fn can_mint_and_burn_liquidity() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000_000);
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Usdc, 1_000_000);

		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(
			Origin::root(),
			asset,
			0,
			PoolState::sqrt_price_at_tick(0),
		));
		System::reset_events();

		// Can open a new position
		assert_ok!(LiquidityProvider::update_position(
			Origin::signed(LP_ACCOUNT.into()),
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

		System::assert_has_event(Event::LiquidityPools(pallet_cf_pools::Event::LiquidityMinted {
			lp: LP_ACCOUNT.into(),
			asset,
			range,
			minted_liquidity: 1_000_000,
			asset_debited: PoolAssetMap::new(4988, 4988),
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_debited: 4988,
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_debited: 4988,
		}));

		assert_eq!(
			LiquidityPools::minted_liqudity(&LP_ACCOUNT.into(), &asset),
			vec![MintedLiquidity {
				range: AmmRange::new(range.lower, range.upper),
				liquidity: 1_000_000,
				fees_acrued: PoolAssetMap::default()
			}]
		);

		// Can mint more liquidity (+1000)
		System::reset_events();
		assert_ok!(LiquidityProvider::update_position(
			Origin::signed(LP_ACCOUNT.into()),
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

		System::assert_has_event(Event::LiquidityPools(pallet_cf_pools::Event::LiquidityMinted {
			lp: LP_ACCOUNT.into(),
			asset,
			range,
			minted_liquidity: 1_000,
			asset_debited: PoolAssetMap::new(5, 5),
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_debited: 5,
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountDebited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_debited: 5,
		}));

		assert_eq!(
			LiquidityPools::minted_liqudity(&LP_ACCOUNT.into(), &asset),
			vec![MintedLiquidity {
				range: AmmRange::new(range.lower, range.upper),
				liquidity: 1_001_000,
				fees_acrued: PoolAssetMap::default()
			}]
		);

		// Can partially burn a liquidity position (-500_000)
		System::reset_events();
		assert_ok!(LiquidityProvider::update_position(
			Origin::signed(LP_ACCOUNT.into()),
			asset,
			range,
			501_000,
		));

		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(997_500)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(997_500)
		);

		System::assert_has_event(Event::LiquidityPools(pallet_cf_pools::Event::LiquidityBurned {
			lp: LP_ACCOUNT.into(),
			asset,
			range,
			burnt_liquidity: 500_000,
			asset_credited: PoolAssetMap::new(2493, 2493),
			fee_yielded: Default::default(),
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_credited: 2493,
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_credited: 2493,
		}));

		assert_eq!(
			LiquidityPools::minted_liqudity(&LP_ACCOUNT.into(), &asset),
			vec![MintedLiquidity {
				range: AmmRange::new(range.lower, range.upper),
				liquidity: 501_000,
				fees_acrued: PoolAssetMap::default()
			}]
		);

		// Can fully burn a position
		System::reset_events();
		assert_ok!(LiquidityProvider::update_position(
			Origin::signed(LP_ACCOUNT.into()),
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

		System::assert_has_event(Event::LiquidityPools(pallet_cf_pools::Event::LiquidityBurned {
			lp: LP_ACCOUNT.into(),
			asset,
			range,
			burnt_liquidity: 501_000,
			asset_credited: PoolAssetMap::new(2_498, 2_498),
			fee_yielded: Default::default(),
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset,
			amount_credited: 2_498,
		}));
		System::assert_has_event(Event::LiquidityProvider(crate::Event::AccountCredited {
			account_id: LP_ACCOUNT.into(),
			asset: Asset::Usdc,
			amount_credited: 2_498,
		}));

		assert_eq!(LiquidityPools::minted_liqudity(&LP_ACCOUNT.into(), &asset), vec![]);
	});
}

#[test]
fn mint_fails_with_insufficient_balance() {
	new_test_ext().execute_with(|| {
		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		assert_ok!(LiquidityPools::new_pool(
			Origin::root(),
			asset,
			0,
			PoolState::sqrt_price_at_tick(0),
		));
		System::reset_events();

		// Can open a new position
		assert_noop!(
			LiquidityProvider::update_position(
				Origin::signed(LP_ACCOUNT.into()),
				asset,
				range,
				1_000_000,
			),
			pallet_cf_pools::Error::<Test>::InsufficientBalance
		);
	});
}

#[test]
fn can_collect_fee() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000_000);
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Usdc, 1_000_000);

		let range = AmmRange::new(-100, 100);
		let asset = Asset::Eth;

		// 50% fee
		assert_ok!(LiquidityPools::new_pool(
			Origin::root(),
			asset,
			500_000u32,
			PoolState::sqrt_price_at_tick(0),
		));

		// Can open a new position
		assert_ok!(LiquidityProvider::update_position(
			Origin::signed(LP_ACCOUNT.into()),
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

		System::reset_events();

		// Trigger swap to acrue some fees
		assert_eq!(
			LiquidityPools::swap(Asset::Eth, Asset::Usdc, U256::from(1_000u128)),
			Ok((U256::from(499u128), U256::from(500u128), U256::from(0u128)))
		);
		assert_eq!(
			LiquidityPools::minted_liqudity(&LP_ACCOUNT.into(), &asset),
			vec![MintedLiquidity {
				range: AmmRange::new(range.lower, range.upper),
				liquidity: 1_000_000,
				fees_acrued: PoolAssetMap::default()
			}]
		);

		// Balance before the collect.
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(995_012)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(995_012)
		);

		// Collect fees acrued for the Liquidity Position.
		assert_ok!(LiquidityProvider::collect_fees(
			Origin::signed(LP_ACCOUNT.into()),
			asset,
			range
		));
		
		System::assert_has_event(Event::LiquidityPools(pallet_cf_pools::Event::FeeCollected {
			lp: LP_ACCOUNT.into(),
			asset,
			range,
			fee_yielded: PoolAssetMap::new(499, 0),
		}));

		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth),
			Some(995_511)
		);
		assert_eq!(
			FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Usdc),
			Some(995_012)
		);

		// Fees has been reset.
		assert_eq!(
			LiquidityPools::minted_liqudity(&LP_ACCOUNT.into(), &asset),
			vec![MintedLiquidity {
				range: AmmRange::new(range.lower, range.upper),
				liquidity: 1_000_000,
				fees_acrued: PoolAssetMap::default()
			}]
		);
	});
}
