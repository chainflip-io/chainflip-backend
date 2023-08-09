use crate::{mock::*, EmergencyWithdrawalAddress, FreeBalances, LiquidityChannelExpiries, LpTTL};

use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	AnyChain, ForeignChainAddress,
};
use cf_primitives::{AccountId, Asset, ForeignChain};

use cf_test_utilities::assert_events_match;
use cf_traits::{
	mocks::{
		address_converter::MockAddressConverter,
		deposit_handler::{LpChannel, MockDepositHandler},
	},
	SetSafeMode,
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::Hooks};

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
		assert_ok!(LiquidityProvider::register_emergency_withdrawal_address(
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
fn deposit_channel_expires() {
	new_test_ext().execute_with(|| {
		assert_ok!(LiquidityProvider::register_emergency_withdrawal_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			EncodedAddress::Eth(Default::default()),
		));
		// Expiry = current (1) + ttl
		let expiry = LpTTL::<Test>::get() + 1;
		let asset = Asset::Eth;
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			asset,
		));

		let (channel_id, deposit_address) = assert_events_match!(Test, RuntimeEvent::LiquidityProvider(crate::Event::LiquidityDepositAddressReady {
			channel_id,
			deposit_address,
			expiry_block,
		}) if expiry_block == expiry => (channel_id, deposit_address));
		let lp_channel = LpChannel {
			deposit_address: MockAddressConverter::try_from_encoded_address(deposit_address.clone()).unwrap(),
			source_asset: asset,
			lp_account: LP_ACCOUNT.into(),
		};

		assert_eq!(
			LiquidityChannelExpiries::<Test>::get(expiry),
			vec![(channel_id, MockAddressConverter::try_from_encoded_address(deposit_address.clone()).unwrap())]
		);
		assert_eq!(
			MockDepositHandler::<AnyChain, Test>::get_liquidity_channels(),
			vec![lp_channel.clone()]
		);

		// Does not expire until expiry
		LiquidityProvider::on_initialize(expiry - 1);
		assert_eq!(
			LiquidityChannelExpiries::<Test>::get(expiry),
			vec![(channel_id, MockAddressConverter::try_from_encoded_address(deposit_address.clone()).unwrap())]
		);
		assert_eq!(
			MockDepositHandler::<AnyChain, Test>::get_liquidity_channels(),
			vec![lp_channel]
		);

		// Expire the address on the expiry block
		LiquidityProvider::on_initialize(expiry);

		assert_eq!(LiquidityChannelExpiries::<Test>::get(expiry), vec![]);
		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			crate::Event::<Test>::LiquidityDepositAddressExpired { address: deposit_address },
		));
		assert!(MockDepositHandler::<AnyChain, Test>::get_liquidity_channels().is_empty());
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

#[test]
fn can_register_and_deregister_emergency_withdrawal_address() {
	new_test_ext().execute_with(|| {
		let account_id = AccountId::from(LP_ACCOUNT);
		let encoded_address = EncodedAddress::Eth([0x01; 20]);
		let decoded_address = ForeignChainAddress::Eth([0x01; 20].into());
		assert!(
			EmergencyWithdrawalAddress::<Test>::get(&account_id, ForeignChain::Ethereum).is_none()
		);

		// Can register EWA
		assert_ok!(LiquidityProvider::register_emergency_withdrawal_address(
			RuntimeOrigin::signed(account_id.clone()),
			encoded_address
		));
		assert_eq!(
			EmergencyWithdrawalAddress::<Test>::get(&account_id, ForeignChain::Ethereum),
			Some(decoded_address.clone())
		);
		// Other chain should be unaffected.
		assert!(
			EmergencyWithdrawalAddress::<Test>::get(&account_id, ForeignChain::Polkadot).is_none()
		);
		assert!(
			EmergencyWithdrawalAddress::<Test>::get(&account_id, ForeignChain::Bitcoin).is_none()
		);

		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			crate::Event::<Test>::EmergencyWithdrawalAddressRegistered {
				account_id: account_id.clone(),
				chain: ForeignChain::Ethereum,
				address: decoded_address,
			},
		));

		// Can reaplce the registered EWA with a new one.
		let encoded_address = EncodedAddress::Eth([0x05; 20]);
		let decoded_address = ForeignChainAddress::Eth([0x05; 20].into());

		assert_ok!(LiquidityProvider::register_emergency_withdrawal_address(
			RuntimeOrigin::signed(account_id.clone()),
			encoded_address,
		));
		assert_eq!(
			EmergencyWithdrawalAddress::<Test>::get(&account_id, ForeignChain::Ethereum),
			Some(decoded_address.clone()),
		);
		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			crate::Event::<Test>::EmergencyWithdrawalAddressRegistered {
				account_id,
				chain: ForeignChain::Ethereum,
				address: decoded_address,
			},
		));
	});
}

#[test]
fn cannot_request_deposit_address_without_registering_emergency_withdrawal_address() {
	new_test_ext().execute_with(|| {
		assert_noop!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
		), crate::Error::<Test>::NoEmergencyWithdrawalAddressRegistered);

		// Register EWA
		assert_ok!(LiquidityProvider::register_emergency_withdrawal_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			EncodedAddress::Eth([0x01; 20])
		));

		// Now the LPer should be able to request deposit channel for assets of the Ethereum chain.
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Eth,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Flip,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Usdc,
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
		), crate::Error::<Test>::NoEmergencyWithdrawalAddressRegistered);
		assert_noop!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT.into()),
			Asset::Dot,
		), crate::Error::<Test>::NoEmergencyWithdrawalAddressRegistered);
	});
}
