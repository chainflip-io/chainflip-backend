use crate::{mock::*, FreeBalances, LiquidityRefundAddress};

use cf_chains::{address::EncodedAddress, ForeignChainAddress};
use cf_primitives::{AccountId, Asset, ForeignChain};

use cf_test_utilities::assert_events_match;
use cf_traits::SetSafeMode;
use frame_support::{assert_noop, assert_ok, error::BadOrigin};

#[test]
fn egress_chain_and_asset_must_match() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				1,
				Asset::Eth,
				EncodedAddress::Dot(Default::default()),
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
				EncodedAddress::Eth(Default::default()),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);

		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(NON_LP_ACCOUNT.into()),
				100,
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			BadOrigin
		);

		assert_ok!(LiquidityProvider::withdraw_asset(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			100,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));

		assert_eq!(FreeBalances::<Test>::get(AccountId::from(LP_ACCOUNT), Asset::Eth), Some(900));
	});
}

#[test]
fn cannot_deposit_and_withdrawal_during_safe_mode() {
	new_test_ext().execute_with(|| {
		FreeBalances::<Test>::insert(AccountId::from(LP_ACCOUNT), Asset::Eth, 1_000);
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			EncodedAddress::Eth(Default::default()),
		));

		// Activate Safe Mode: Code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// Cannot request deposit address during Code red.
		assert_noop!(
			LiquidityProvider::request_liquidity_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				Asset::Eth,
				0
			),
			crate::Error::<Test>::LiquidityDepositDisabled,
		);

		// Cannot withdraw liquidity during Code red.
		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				100,
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			crate::Error::<Test>::WithdrawalsDisabled,
		);

		// Safe mode is now Code Green
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();

		// Deposit and withdrawal can now work as per normal.
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
			0
		));

		assert_ok!(LiquidityProvider::withdraw_asset(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			100,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));
	});
}

#[test]
fn can_register_and_deregister_liquidity_refund_address() {
	new_test_ext().execute_with(|| {
		let account_id = AccountId::from(LP_ACCOUNT);
		let encoded_address = EncodedAddress::Eth([0x01; 20]);
		let decoded_address = ForeignChainAddress::Eth([0x01; 20].into());
		assert!(LiquidityRefundAddress::<Test>::get(&account_id, ForeignChain::Ethereum).is_none());

		// Can register EWA
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(account_id.clone()),
			encoded_address
		));
		assert_eq!(
			LiquidityRefundAddress::<Test>::get(&account_id, ForeignChain::Ethereum),
			Some(decoded_address.clone())
		);
		// Other chain should be unaffected.
		assert!(LiquidityRefundAddress::<Test>::get(&account_id, ForeignChain::Polkadot).is_none());
		assert!(LiquidityRefundAddress::<Test>::get(&account_id, ForeignChain::Bitcoin).is_none());

		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			crate::Event::<Test>::LiquidityRefundAddressRegistered {
				account_id: account_id.clone(),
				chain: ForeignChain::Ethereum,
				address: decoded_address,
			},
		));

		// Can reaplce the registered EWA with a new one.
		let encoded_address = EncodedAddress::Eth([0x05; 20]);
		let decoded_address = ForeignChainAddress::Eth([0x05; 20].into());

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(account_id.clone()),
			encoded_address,
		));
		assert_eq!(
			LiquidityRefundAddress::<Test>::get(&account_id, ForeignChain::Ethereum),
			Some(decoded_address.clone()),
		);
		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			crate::Event::<Test>::LiquidityRefundAddressRegistered {
				account_id,
				chain: ForeignChain::Ethereum,
				address: decoded_address,
			},
		));
	});
}

#[test]
fn cannot_request_deposit_address_without_registering_liquidity_refund_address() {
	new_test_ext().execute_with(|| {
		assert_noop!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
			0,
		), crate::Error::<Test>::NoLiquidityRefundAddressRegistered);

		// Register EWA
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			EncodedAddress::Eth([0x01; 20])
		));

		// Now the LPer should be able to request deposit channel for assets of the Ethereum chain.
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
			0,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Flip,
			0,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Usdc,
			0,
		));
		assert_events_match!(Test, RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			..
		}) => ());
		// Requesting deposit address for other chains will fail.
		assert_noop!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Btc,
			0,
		), crate::Error::<Test>::NoLiquidityRefundAddressRegistered);
		assert_noop!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Dot,
			0,
		), crate::Error::<Test>::NoLiquidityRefundAddressRegistered);
	});
}

#[test]
fn deposit_address_ready_event_contain_correct_boost_fee_value() {
	new_test_ext().execute_with(|| {
		const BOOST_FEE1: u16 = 0;
		const BOOST_FEE2: u16 = 50;
		const BOOST_FEE3: u16 = 100;

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			EncodedAddress::Eth([0x01; 20])
		));

		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
			BOOST_FEE1,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Flip,
			BOOST_FEE2,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Usdc,
			BOOST_FEE3,
		));
		assert_events_match!(Test, RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			boost_fee: BOOST_FEE1,
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			boost_fee: BOOST_FEE2,
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			boost_fee: BOOST_FEE3,
			..
		}) => ());
	});
}
