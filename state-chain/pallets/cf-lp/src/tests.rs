use crate::{mock::*, CcmIntentExpiries, FreeBalances, LpTTL};

use cf_chains::address::ForeignChainAddress;
use cf_primitives::{AccountId, Asset, ForeignChain};
use cf_traits::{mocks::system_state_info::MockSystemStateInfo, SystemStateInfo};
use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::Hooks};

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
fn ccm_intents_expires() {
	new_test_ext().execute_with(|| {
		// Expiry = current (1) + ttl
		let expiry = LpTTL::<Test>::get() + 1;
		let asset = Asset::Eth;
		assert_ok!(LiquidityProvider::request_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
		));
		if let RuntimeEvent::LiquidityProvider(crate::Event::<Test>::DepositAddressReady {
			intent_id,
			ingress_address,
		}) = &System::events().last().unwrap().event
		{
			assert_eq!(
				CcmIntentExpiries::<Test>::get(expiry),
				vec![(*intent_id, ForeignChain::from(asset), ingress_address.clone())]
			);

			// Does not expire until expiry
			LiquidityProvider::on_initialize(expiry - 1);

			assert_eq!(
				CcmIntentExpiries::<Test>::get(expiry),
				vec![(*intent_id, ForeignChain::from(asset), ingress_address.clone())]
			);

			// Expire the address
			LiquidityProvider::on_initialize(expiry);
			assert_eq!(CcmIntentExpiries::<Test>::get(expiry), vec![]);
			System::assert_last_event(RuntimeEvent::LiquidityProvider(
				crate::Event::<Test>::DepositAddressExpired { address: ingress_address.clone() },
			));
		} else {
			panic!("Deposit address must be be emited as an event.");
		}
	});
}

#[test]
fn can_set_lp_ttl() {
	new_test_ext().execute_with(|| {
		assert_eq!(LpTTL::<Test>::get(), 1_200);
		assert_ok!(LiquidityProvider::set_lp_ttl(RuntimeOrigin::root(), 10));
		assert_eq!(LpTTL::<Test>::get(), 10);
	});
}
