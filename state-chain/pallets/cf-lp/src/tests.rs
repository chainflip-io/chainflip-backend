use crate::{mock::*, Error, Event, LiquidityRefundAddress};

use cf_chains::{address::EncodedAddress, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount, ForeignChain};

use cf_test_utilities::assert_events_match;
use cf_traits::{
	mocks::swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	AccountRoleRegistry, BalanceApi, Chainflip, SetSafeMode, SwapOutputAction, SwapRequestType,
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::OriginTrait};
use sp_runtime::AccountId32;

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
		MockBalanceApi::insert_balance(LP_ACCOUNT.into(), 1_000);
		MockBalanceApi::insert_balance(NON_LP_ACCOUNT.into(), 1_000);

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
	});
}

#[test]
fn liquidity_providers_can_move_assets_internally() {
	new_test_ext().execute_with(|| {
		const BALANCE_LP_1: AssetAmount = 1_000;
		const TRANSFER_AMOUNT: AssetAmount = 100;

		MockBalanceApi::insert_balance(LP_ACCOUNT.into(), BALANCE_LP_1);

		// Cannot move assets to a non-LP account.
		assert_noop!(
			LiquidityProvider::transfer_asset(
				RuntimeOrigin::signed((LP_ACCOUNT).into()),
				TRANSFER_AMOUNT,
				Asset::Eth,
				NON_LP_ACCOUNT,
			),
			Error::<Test>::DestinationAccountNotLiquidityProvider
		);

		// Cannot transfer assets to the same account.
		assert_noop!(
			LiquidityProvider::transfer_asset(
				RuntimeOrigin::signed((LP_ACCOUNT).into()),
				TRANSFER_AMOUNT,
				Asset::Eth,
				LP_ACCOUNT,
			),
			Error::<Test>::CannotTransferToOriginAccount
		);

		assert_ok!(LiquidityProvider::transfer_asset(
			RuntimeOrigin::signed((LP_ACCOUNT).into()),
			TRANSFER_AMOUNT,
			Asset::Eth,
			LP_ACCOUNT_2,
		));

		System::assert_last_event(RuntimeEvent::LiquidityProvider(Event::AssetTransferred {
			from: LP_ACCOUNT,
			to: LP_ACCOUNT_2,
			asset: Asset::Eth,
			amount: TRANSFER_AMOUNT,
		}));
	});
}

#[test]
fn cannot_deposit_and_withdrawal_during_safe_mode() {
	new_test_ext().execute_with(|| {
		MockBalanceApi::insert_balance(LP_ACCOUNT.into(), 1_000);
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
		let encoded_address = EncodedAddress::Eth([0x01; 20]);
		let decoded_address = ForeignChainAddress::Eth([0x01; 20].into());
		assert!(LiquidityRefundAddress::<Test>::get(&LP_ACCOUNT, ForeignChain::Ethereum).is_none());

		// Can register EWA
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			encoded_address
		));
		assert_eq!(
			LiquidityRefundAddress::<Test>::get(&LP_ACCOUNT, ForeignChain::Ethereum),
			Some(decoded_address.clone())
		);
		// Other chain should be unaffected.
		assert!(LiquidityRefundAddress::<Test>::get(&LP_ACCOUNT, ForeignChain::Polkadot).is_none());
		assert!(LiquidityRefundAddress::<Test>::get(&LP_ACCOUNT, ForeignChain::Bitcoin).is_none());

		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			Event::<Test>::LiquidityRefundAddressRegistered {
				account_id: LP_ACCOUNT,
				chain: ForeignChain::Ethereum,
				address: decoded_address,
			},
		));

		// Can replace the registered EWA with a new one.
		let encoded_address = EncodedAddress::Eth([0x05; 20]);
		let decoded_address = ForeignChainAddress::Eth([0x05; 20].into());

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			encoded_address,
		));
		assert_eq!(
			LiquidityRefundAddress::<Test>::get(&LP_ACCOUNT, ForeignChain::Ethereum),
			Some(decoded_address.clone()),
		);
		System::assert_last_event(RuntimeEvent::LiquidityProvider(
			Event::<Test>::LiquidityRefundAddressRegistered {
				account_id: LP_ACCOUNT,
				chain: ForeignChain::Ethereum,
				address: decoded_address,
			},
		));
	});
}

#[test]
fn cannot_request_deposit_address_without_registering_liquidity_refund_address() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityProvider::request_liquidity_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				Asset::Eth,
				0,
			),
			crate::Error::<Test>::NoLiquidityRefundAddressRegistered
		);

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
		assert_events_match!(Test, RuntimeEvent::LiquidityProvider(Event::LiquidityDepositAddressReady {
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(Event::LiquidityDepositAddressReady {
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(Event::LiquidityDepositAddressReady {
			..
		}) => ());
		// Requesting deposit address for other chains will fail.
		assert_noop!(
			LiquidityProvider::request_liquidity_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				Asset::Btc,
				0,
			),
			crate::Error::<Test>::NoLiquidityRefundAddressRegistered
		);
		assert_noop!(
			LiquidityProvider::request_liquidity_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT.into()),
				Asset::Dot,
				0,
			),
			crate::Error::<Test>::NoLiquidityRefundAddressRegistered
		);
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
		assert_events_match!(Test, RuntimeEvent::LiquidityProvider(Event::LiquidityDepositAddressReady {
			boost_fee: BOOST_FEE1,
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(Event::LiquidityDepositAddressReady {
			boost_fee: BOOST_FEE2,
			..
		}) => (),
		RuntimeEvent::LiquidityProvider(Event::LiquidityDepositAddressReady {
			boost_fee: BOOST_FEE3,
			..
		}) => ());
	});
}

#[test]
fn account_registration_and_deregistration() {
	new_test_ext().execute_with(|| {
		const DEPOSIT_AMOUNT: AssetAmount = 1_000;

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_liquidity_provider(OriginTrait::signed(LP_ACCOUNT))
		.expect("LP_ACCOUNT registered at genesis.");
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			OriginTrait::signed(LP_ACCOUNT),
			EncodedAddress::Eth([0x01; 20])
		));

		MockBalanceApi::credit_account(&LP_ACCOUNT, Asset::Eth, DEPOSIT_AMOUNT);

		assert_noop!(
			LiquidityProvider::deregister_lp_account(OriginTrait::signed(LP_ACCOUNT)),
			Error::<Test>::FundsRemaining,
		);

		assert_ok!(LiquidityProvider::withdraw_asset(
			OriginTrait::signed(LP_ACCOUNT),
			DEPOSIT_AMOUNT,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));

		assert_ok!(MockIngressEgressBoostApi::set_boost_funds(100));

		assert_noop!(
			LiquidityProvider::deregister_lp_account(OriginTrait::signed(LP_ACCOUNT)),
			Error::<Test>::BoostedFundsRemaining,
		);

		assert_ok!(MockIngressEgressBoostApi::remove_boost_funds(100));

		assert_ok!(
			LiquidityProvider::deregister_lp_account(OriginTrait::signed(LP_ACCOUNT)),
		);

		assert!(
			LiquidityRefundAddress::<Test>::get(&LP_ACCOUNT, ForeignChain::Ethereum).is_none()
		);

		assert!(MockBalanceApi::free_balances(&LP_ACCOUNT)
			.iter()
			.all(|(_, amount)| *amount == 0));
	});
}

#[test]
fn internal_swap_checks() {
	new_test_ext().execute_with(|| {

		const NOT_LP_ACCOUNT: u64 = 11;
		const INPUT_AMOUNT: AssetAmount = 1_000;

		// Must be an LP:
		LiquidityProvider::internal_swap(
			RuntimeOrigin::signed(NOT_LP_ACCOUNT),
			INPUT_AMOUNT,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		).unwrap_err();

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::ensure_liquidity_provider(OriginTrait::signed(
			LP_ACCOUNT,
		))
		.expect("LP_ACCOUNT registered at genesis.");

		// Must register a refund address
		assert_noop!(LiquidityProvider::internal_swap(
			RuntimeOrigin::signed(LP_ACCOUNT),
			INPUT_AMOUNT,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		), Error::<Test>::NoLiquidityRefundAddressRegistered);

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			OriginTrait::signed(LP_ACCOUNT),
			EncodedAddress::Eth([0x01; 20])
		));

		// Must have sufficient balance:
		assert_noop!(LiquidityProvider::internal_swap(
			RuntimeOrigin::signed(LP_ACCOUNT),
			INPUT_AMOUNT,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		), Error::<Test>::InsufficientBalance);

		MockBalanceApi::credit_account(&LP_ACCOUNT, Asset::Eth, INPUT_AMOUNT);

		// Now the extrinsic should succeed resulting in a swap request getting recorded:
		assert_ok!(LiquidityProvider::internal_swap(
			RuntimeOrigin::signed(LP_ACCOUNT),
			INPUT_AMOUNT,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		));

		assert_eq!(MockSwapRequestHandler::<Test>::get_swap_requests(),
			vec![MockSwapRequest {
				input_asset: Asset::Eth,
				output_asset: Asset::Flip,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular {
					output_action: SwapOutputAction::CreditOnChain { account_id: LP_ACCOUNT }
				},
				broker_fees: Default::default(),
				origin: cf_chains::SwapOrigin::OnChainAccount(LP_ACCOUNT)
			}]
		);

	});
}
