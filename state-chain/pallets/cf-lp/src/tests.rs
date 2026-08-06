// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
	mock::*, AggStats, DeltaStats, Error, Event, LpAggStats, LpDeltaStats, Pallet, PalletSafeMode,
	StatsLastUpdatedAt, StatsUpdateCursor, WindowedEma, ALPHA_HALF_LIFE_1_DAY,
	ALPHA_HALF_LIFE_30_DAYS, ALPHA_HALF_LIFE_7_DAYS, EMA_PRUNE_THRESHOLD_USD,
	STATS_UPDATE_INTERVAL_IN_BLOCKS,
};
use std::collections::BTreeMap;

use cf_chains::{address::EncodedAddress, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount, ForeignChain, SwapRequestId, SECONDS_PER_BLOCK};

use cf_test_utilities::assert_events_match;
use cf_traits::{
	mocks::{
		balance_api::MockRefundAddressRegistry,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
	AccountRoleRegistry, BalanceApi, Chainflip,
	ExpiryBehaviour::RefundIfExpires,
	LpStatsApi, PriceLimitsAndExpiry, RefundAddressRegistry, SafeMode, SetSafeMode,
	SwapOutputAction, SwapRequestType,
};
use frame_support::{assert_err, assert_noop, assert_ok, error::BadOrigin, traits::OriginTrait};
use sp_runtime::FixedU128;

#[test]
fn egress_chain_and_asset_must_match() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
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
		MockBalanceApi::insert_balance(LP_ACCOUNT, Asset::Eth, 1_000);
		MockBalanceApi::insert_balance(NON_LP_ACCOUNT, Asset::Eth, 1_000);

		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
				100,
				Asset::Dot,
				EncodedAddress::Eth(Default::default()),
			),
			crate::Error::<Test>::InvalidEgressAddress
		);

		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(NON_LP_ACCOUNT),
				100,
				Asset::Eth,
				EncodedAddress::Eth(Default::default()),
			),
			BadOrigin
		);

		assert_ok!(LiquidityProvider::withdraw_asset(
			RuntimeOrigin::signed(LP_ACCOUNT),
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

		MockBalanceApi::insert_balance(LP_ACCOUNT, Asset::Eth, BALANCE_LP_1);

		// Cannot move assets to a non-LP account.
		assert_noop!(
			LiquidityProvider::transfer_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
				TRANSFER_AMOUNT,
				Asset::Eth,
				NON_LP_ACCOUNT,
			),
			Error::<Test>::DestinationAccountNotLiquidityProvider
		);

		// Cannot transfer assets to the same account.
		assert_noop!(
			LiquidityProvider::transfer_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
				TRANSFER_AMOUNT,
				Asset::Eth,
				LP_ACCOUNT,
			),
			Error::<Test>::CannotTransferToOriginAccount
		);

		assert_err!(
			LiquidityProvider::transfer_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
				TRANSFER_AMOUNT,
				Asset::Eth,
				LP_ACCOUNT_2,
			),
			Error::<Test>::NoLiquidityRefundAddressRegistered
		);

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT_2),
			EncodedAddress::Eth(Default::default())
		));

		assert_ok!(LiquidityProvider::transfer_asset(
			RuntimeOrigin::signed(LP_ACCOUNT),
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
		MockBalanceApi::insert_balance(LP_ACCOUNT, Asset::Eth, 1_000);
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			EncodedAddress::Eth(Default::default()),
		));

		// Activate Safe Mode: Code red
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();

		// Cannot request deposit address during Code red.
		assert_noop!(
			LiquidityProvider::request_liquidity_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT),
				Asset::Eth,
				0
			),
			crate::Error::<Test>::LiquidityDepositDisabled,
		);

		// Cannot withdraw liquidity during Code red.
		assert_noop!(
			LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
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
			RuntimeOrigin::signed(LP_ACCOUNT),
			Asset::Eth,
			0
		));

		assert_ok!(LiquidityProvider::withdraw_asset(
			RuntimeOrigin::signed(LP_ACCOUNT),
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
		assert!(MockRefundAddressRegistry::get_refund_address(&LP_ACCOUNT, ForeignChain::Ethereum)
			.is_none());

		// Can register EWA
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			encoded_address
		));
		assert_eq!(
			MockRefundAddressRegistry::get_refund_address(&LP_ACCOUNT, ForeignChain::Ethereum),
			Some(decoded_address.clone())
		);
		// Other chain should be unaffected.
		assert!(MockRefundAddressRegistry::get_refund_address(&LP_ACCOUNT, ForeignChain::Polkadot)
			.is_none());
		assert!(MockRefundAddressRegistry::get_refund_address(&LP_ACCOUNT, ForeignChain::Bitcoin)
			.is_none());

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
			MockRefundAddressRegistry::get_refund_address(&LP_ACCOUNT, ForeignChain::Ethereum),
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
				RuntimeOrigin::signed(LP_ACCOUNT),
				Asset::Eth,
				0,
			),
			crate::Error::<Test>::NoLiquidityRefundAddressRegistered
		);

		// Register EWA
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			EncodedAddress::Eth([0x01; 20])
		));

		// Now the LPer should be able to request deposit channel for assets of the Ethereum chain.
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			Asset::Eth,
			0,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			Asset::Flip,
			0,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
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
				RuntimeOrigin::signed(LP_ACCOUNT),
				Asset::Btc,
				0,
			),
			crate::Error::<Test>::NoLiquidityRefundAddressRegistered
		);
		assert_noop!(
			LiquidityProvider::request_liquidity_deposit_address(
				RuntimeOrigin::signed(LP_ACCOUNT),
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
			RuntimeOrigin::signed(LP_ACCOUNT),
			EncodedAddress::Eth([0x01; 20])
		));

		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			Asset::Eth,
			BOOST_FEE1,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
			Asset::Flip,
			BOOST_FEE2,
		));
		assert_ok!(LiquidityProvider::request_liquidity_deposit_address(
			RuntimeOrigin::signed(LP_ACCOUNT),
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

		assert_ok!(LiquidityProvider::withdraw_asset(
			OriginTrait::signed(LP_ACCOUNT),
			DEPOSIT_AMOUNT,
			Asset::Eth,
			EncodedAddress::Eth(Default::default()),
		));

		assert_ok!(
			LiquidityProvider::deregister_lp_account(OriginTrait::signed(LP_ACCOUNT)),
		);

		assert!(
			MockRefundAddressRegistry::get_refund_address(&LP_ACCOUNT, ForeignChain::Ethereum).is_none()
		);

		assert!(MockBalanceApi::free_balances(&LP_ACCOUNT)
			.iter()
			.all(|(_, amount)| *amount == 0));
	});
}

#[test]
fn schedule_swap_checks() {
	new_test_ext().execute_with(|| {

		const NOT_LP_ACCOUNT: u64 = 11;
		const INPUT_AMOUNT: AssetAmount = 1_000;
		const BELLOW_MINIMUM_AMOUNT: AssetAmount = MINIMUM_DEPOSIT - 1;

		// Must be above minimum deposit amount:
		assert_noop!(
			LiquidityProvider::schedule_swap(
				RuntimeOrigin::signed(LP_ACCOUNT),
				BELLOW_MINIMUM_AMOUNT,
				Asset::Eth,
				Asset::Flip,
				0,
				Default::default(),
				None,
			),
			Error::<Test>::InternalSwapBelowMinimumDepositAmount
		);

		// Must be an LP:
		LiquidityProvider::schedule_swap(
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
		assert_noop!(LiquidityProvider::schedule_swap(
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
		assert_noop!(LiquidityProvider::schedule_swap(
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
		assert_ok!(LiquidityProvider::schedule_swap(
			RuntimeOrigin::signed(LP_ACCOUNT),
			INPUT_AMOUNT,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		));

		assert_eq!(MockSwapRequestHandler::<Test>::get_swap_requests(),
			BTreeMap::from([(SwapRequestId(0), MockSwapRequest {
				input_asset: Asset::Eth,
				output_asset: Asset::Flip,
				input_amount: INPUT_AMOUNT,
				swap_type: SwapRequestType::Regular {
					output_action: SwapOutputAction::CreditOnChain { account_id: LP_ACCOUNT }
				},
				broker_fees: Default::default(),
				origin: cf_chains::SwapOrigin::OnChainAccount(LP_ACCOUNT),
				remaining_input_amount: INPUT_AMOUNT,
				accumulated_output_amount: 0,
				price_limits_and_expiry: Some(PriceLimitsAndExpiry {
					expiry_behaviour: RefundIfExpires {
						retry_duration: 0,
						refund_address: cf_chains::AccountOrAddress::InternalAccount(LP_ACCOUNT),
						refund_ccm_metadata: None,
					},
					min_price: Default::default(),
					max_oracle_price_slippage: None,
				}),
				dca_params: None,
			})])
		);

	});
}

#[test]
fn safe_mode_prevents_internal_swaps() {
	new_test_ext().execute_with(|| {
		const AMOUNT: AssetAmount = 1000;

		MockBalanceApi::credit_account(&LP_ACCOUNT, Asset::Eth, AMOUNT);

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			OriginTrait::signed(LP_ACCOUNT),
			EncodedAddress::Eth([0x01; 20])
		));

		let schedule_swap = || {
			LiquidityProvider::schedule_swap(
				RuntimeOrigin::signed(LP_ACCOUNT),
				AMOUNT,
				Asset::Eth,
				Asset::Flip,
				0,
				Default::default(),
				None,
			)
		};

		// LP should not be able to schedule an internal swaps due to safe mode:
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			liquidity_provider: PalletSafeMode {
				internal_swaps_enabled: false,
				..PalletSafeMode::code_green()
			},
		});

		assert_err!(schedule_swap(), Error::<Test>::InternalSwapsDisabled);

		// As soon as we enable internal swaps the LP should be able to schedule a swap:
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			liquidity_provider: PalletSafeMode::code_green(),
		});

		assert_ok!(schedule_swap());
	});
}

/// Alpha half-life factor for exponential moving averages.
/// Calculated as: alpha = 1 - e^(-ln2 * sampling_interval / half_life_period)
fn expected_alpha_half_life(days: u32) -> FixedU128 {
	use frame_support::sp_runtime::Perbill;
	let decay_factor: f64 = (STATS_UPDATE_INTERVAL_IN_BLOCKS as f64) * (SECONDS_PER_BLOCK as f64) /
		((days as f64) * 24.0 * 3600.0);
	let exp_part: f64 = -std::f64::consts::LN_2 * decay_factor;
	FixedU128::from_perbill(Perbill::from_float(1.0f64 - exp_part.exp()))
}

/// Computes expected EMA using f64
/// EMA_t = alpha * new_sample + (1 - alpha) * EMA_(t-1)
fn expected_ema(prev: f64, delta: f64, half_life_days: u32) -> f64 {
	let alpha = expected_alpha_half_life(half_life_days).to_float();
	alpha * delta + (1.0f64 - alpha) * prev
}

/// Convert FixedU128 to float, NB we use 6 decimal places for USD throughout the tests
fn fixed_u128_to_f64(val: FixedU128) -> f64 {
	FixedU128::from_rational(val.into_inner(), 1_000_000u128).to_float()
}
fn is_within_tiny_error(actual: f64, expected: f64) -> bool {
	(expected - actual).abs() < 0.00001f64
}

#[test]
fn check_ema_alpha_constants_are_correct() {
	let expected_1day = expected_alpha_half_life(1);
	assert_eq!(ALPHA_HALF_LIFE_1_DAY, expected_1day);

	let expected_7days = expected_alpha_half_life(7);
	assert_eq!(ALPHA_HALF_LIFE_7_DAYS, expected_7days);

	let expected_30days = expected_alpha_half_life(30);
	assert_eq!(ALPHA_HALF_LIFE_30_DAYS, expected_30days);
}

#[test]
fn on_limit_order_filled_updates_delta_stats() {
	new_test_ext().execute_with(|| {
		use sp_runtime::FixedU128;

		const USD_AMOUNT: AssetAmount = 1_500_000;

		// Pre-seed LpAggStats entries so on_limit_order_filled accrues into LpDeltaStats instead
		// of eagerly seeding a new entry (that path is covered by
		// `on_limit_order_filled_seeds_new_lp_agg_stats_entry_immediately`).
		LpAggStats::<Test>::insert(LP_ACCOUNT, Asset::Eth, AggStats::default());
		LpAggStats::<Test>::insert(LP_ACCOUNT_2, Asset::Eth, AggStats::default());

		// round 1
		assert!(LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth).is_none());
		for _ in 0..3 {
			LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, USD_AMOUNT);
		}

		let deltas_1 = LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth).unwrap();
		assert_eq!(deltas_1.limit_orders_swap_usd_volume, FixedU128::from_inner(USD_AMOUNT * 3));

		// round2
		assert!(LpDeltaStats::<Test>::get(LP_ACCOUNT_2, Asset::Eth).is_none());
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT_2, &Asset::Eth, USD_AMOUNT);

		let deltas_2 = LpDeltaStats::<Test>::get(LP_ACCOUNT_2, Asset::Eth).unwrap();

		assert_eq!(deltas_2.limit_orders_swap_usd_volume, FixedU128::from_inner(USD_AMOUNT));
	});
}
#[test]
fn update_agg_stats_updates_correctly() {
	new_test_ext().execute_with(|| {
		use sp_runtime::FixedU128;

		let pre_existing_eth_stats = AggStats::new(DeltaStats {
			limit_orders_swap_usd_volume: FixedU128::from_inner(1_000_000_000u128),
		}); // Avg: 1000 USD

		LpAggStats::<Test>::insert(LP_ACCOUNT, Asset::Eth, pre_existing_eth_stats);

		// on_limit_order_filled for LP_ACCOUNT with pre-existing AggStats: accrues a delta, does
		// not touch LpAggStats directly.
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 700_000_000u128); // 700 usd
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 700_000_000u128); // 700 usd
		assert_eq!(
			LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth)
				.unwrap()
				.limit_orders_swap_usd_volume,
			FixedU128::from_inner(1_400_000_000u128)
		);

		// on_limit_order_filled for LP_ACCOUNT_2 (no pre-existing AggStats): seeds a new entry
		// immediately, unblended.
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT_2, &Asset::Flip, 500_000_000u128); // 500 usd
		let lp2_agg_stats = LpAggStats::<Test>::get(LP_ACCOUNT_2, Asset::Flip).unwrap();
		assert_eq!(fixed_u128_to_f64(lp2_agg_stats.avg_limit_usd_volume.one_day), 500f64);
		assert_eq!(fixed_u128_to_f64(lp2_agg_stats.avg_limit_usd_volume.seven_days), 500f64);
		assert_eq!(fixed_u128_to_f64(lp2_agg_stats.avg_limit_usd_volume.thirty_days), 500f64);
		assert_eq!(LpDeltaStats::<Test>::get(LP_ACCOUNT_2, Asset::Flip), None);

		// Call the update function and verify that LP_ACCOUNT's pre-existing entry decays
		// correctly and its delta stats are deleted after the update. Ample weight so the whole
		// (tiny) pass completes in a single call.
		LiquidityProvider::update_agg_stats(
			1,
			None,
			frame_support::weights::Weight::from_parts(u64::MAX, u64::MAX),
		);
		assert_eq!(LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth), None);

		let lp1_agg_stats = LpAggStats::<Test>::get(LP_ACCOUNT, Asset::Eth).unwrap();
		assert!(is_within_tiny_error(
			fixed_u128_to_f64(lp1_agg_stats.avg_limit_usd_volume.one_day),
			expected_ema(
				fixed_u128_to_f64(pre_existing_eth_stats.avg_limit_usd_volume.one_day),
				1400f64,
				1u32
			)
		));
		assert!(is_within_tiny_error(
			fixed_u128_to_f64(lp1_agg_stats.avg_limit_usd_volume.seven_days),
			expected_ema(
				fixed_u128_to_f64(pre_existing_eth_stats.avg_limit_usd_volume.seven_days),
				1400f64,
				7u32
			)
		));
		assert!(is_within_tiny_error(
			fixed_u128_to_f64(lp1_agg_stats.avg_limit_usd_volume.thirty_days),
			expected_ema(
				fixed_u128_to_f64(pre_existing_eth_stats.avg_limit_usd_volume.thirty_days),
				1400f64,
				30u32
			)
		));

		// LP_ACCOUNT_2's freshly-seeded entry already exists in LpAggStats by the time
		// update_agg_stats() runs, so it's included in the same iter() pass: with no pending
		// delta, it decays toward zero like any other existing entry with no activity this
		// period (same as LP_ACCOUNT's entry would if it had no delta).
		let lp2_agg_stats = LpAggStats::<Test>::get(LP_ACCOUNT_2, Asset::Flip).unwrap();
		assert!(is_within_tiny_error(
			fixed_u128_to_f64(lp2_agg_stats.avg_limit_usd_volume.one_day),
			expected_ema(500f64, 0f64, 1u32)
		));
		assert!(is_within_tiny_error(
			fixed_u128_to_f64(lp2_agg_stats.avg_limit_usd_volume.seven_days),
			expected_ema(500f64, 0f64, 7u32)
		));
		assert!(is_within_tiny_error(
			fixed_u128_to_f64(lp2_agg_stats.avg_limit_usd_volume.thirty_days),
			expected_ema(500f64, 0f64, 30u32)
		));
	});
}

#[test]
fn on_limit_order_filled_seeds_new_lp_agg_stats_entry_immediately() {
	new_test_ext().execute_with(|| {
		assert!(LpAggStats::<Test>::get(LP_ACCOUNT, Asset::Eth).is_none());

		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 1_000_000_000u128); // 1000 usd

		// The entry exists immediately, without update_agg_stats() ever running.
		let agg_stats = LpAggStats::<Test>::get(LP_ACCOUNT, Asset::Eth).unwrap();
		assert_eq!(fixed_u128_to_f64(agg_stats.avg_limit_usd_volume.one_day), 1000f64);
		assert_eq!(fixed_u128_to_f64(agg_stats.avg_limit_usd_volume.seven_days), 1000f64);
		assert_eq!(fixed_u128_to_f64(agg_stats.avg_limit_usd_volume.thirty_days), 1000f64);
		// No delta was recorded for this first fill — it was consumed directly into the seed.
		assert_eq!(LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth), None);

		// A second fill for the SAME (Lp, Asset), now that an entry exists, accrues as a delta
		// instead of overwriting the seed.
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 500_000_000u128); // 500 usd
		assert_eq!(
			LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth)
				.unwrap()
				.limit_orders_swap_usd_volume,
			FixedU128::from_inner(500_000_000u128)
		);
		// The seeded aggregate is unchanged until the next update_agg_stats() pass.
		assert_eq!(
			LpAggStats::<Test>::get(LP_ACCOUNT, Asset::Eth)
				.unwrap()
				.avg_limit_usd_volume
				.one_day,
			FixedU128::from_inner(1_000_000_000u128)
		);
	});
}

#[test]
fn update_agg_stats_prunes_below_threshold() {
	new_test_ext().execute_with(|| {
		use sp_runtime::FixedU128;

		let below_threshold = AggStats {
			avg_limit_usd_volume: WindowedEma {
				one_day: FixedU128::from_inner(5_000_000),
				seven_days: FixedU128::from_inner(5_000_000),
				thirty_days: FixedU128::from_inner(5_000_000),
			},
		};
		let above_threshold = AggStats {
			avg_limit_usd_volume: WindowedEma {
				one_day: FixedU128::from_inner(20_000_000),
				seven_days: FixedU128::from_inner(20_000_000),
				thirty_days: FixedU128::from_inner(20_000_000),
			},
		};

		LpAggStats::<Test>::insert(LP_ACCOUNT, Asset::Eth, below_threshold);
		LpAggStats::<Test>::insert(LP_ACCOUNT_2, Asset::Flip, above_threshold);

		// Ample weight so the whole (tiny) pass completes in a single call.
		LiquidityProvider::update_agg_stats(
			1,
			None,
			frame_support::weights::Weight::from_parts(u64::MAX, u64::MAX),
		);

		assert!(LpAggStats::<Test>::get(LP_ACCOUNT, Asset::Eth).is_none());
		assert!(LpAggStats::<Test>::get(LP_ACCOUNT_2, Asset::Flip).is_some());

		let lp2_stats = LpAggStats::<Test>::get(LP_ACCOUNT_2, Asset::Flip).unwrap();
		assert!(
			lp2_stats.avg_limit_usd_volume.pruning_weighted_score() >=
				FixedU128::from_inner(EMA_PRUNE_THRESHOLD_USD)
		);
	});
}

#[test]
fn update_agg_stats_resumes_across_multiple_capped_calls() {
	new_test_ext().execute_with(|| {
		use crate::weights::WeightInfo as _;

		const NUM_LPS: u64 = 5;
		for lp in 0..NUM_LPS {
			// Two fills per Lp: the first seeds the entry, the second leaves a pending delta for
			// update_agg_stats to apply.
			LiquidityProvider::on_limit_order_filled(&lp, &Asset::Eth, 1_000_000_000u128); // 1000 usd
			LiquidityProvider::on_limit_order_filled(&lp, &Asset::Eth, 200_000_000u128); // 200 usd
		}
		assert_eq!(LpAggStats::<Test>::iter().count(), NUM_LPS as usize);

		// Budget exactly one item's worth of weight per call (computed from the real WeightInfo,
		// so this test stays correct however that weight is defined) to force the pass to span
		// multiple calls. Each of the first NUM_LPS calls processes exactly one entry; the pass
		// only recognises it's exhausted (and clears the cursor) on the following call, once
		// `iter.next()` comes back empty — so completion takes NUM_LPS + 1 calls, not NUM_LPS.
		let per_item_weight = <Test as crate::Config>::WeightInfo::update_agg_stats_item();

		let mut cursor: Option<Vec<u8>> = None;
		let mut calls = 0u64;
		loop {
			calls += 1;
			assert!(calls <= NUM_LPS + 1, "pass should complete within NUM_LPS + 1 calls");
			LiquidityProvider::update_agg_stats(calls, cursor.clone(), per_item_weight);
			cursor = StatsUpdateCursor::<Test>::get();
			if cursor.is_none() {
				break;
			}
		}
		assert_eq!(calls, NUM_LPS + 1, "the pass should take one extra call to detect completion");

		for lp in 0..NUM_LPS {
			assert_eq!(LpDeltaStats::<Test>::get(lp, Asset::Eth), None);
			let agg_stats = LpAggStats::<Test>::get(lp, Asset::Eth).unwrap();
			assert!(is_within_tiny_error(
				fixed_u128_to_f64(agg_stats.avg_limit_usd_volume.one_day),
				expected_ema(1000f64, 200f64, 1u32)
			));
		}
	});
}

#[test]
fn update_agg_stats_tolerates_new_entry_inserted_mid_pass() {
	new_test_ext().execute_with(|| {
		use crate::weights::WeightInfo as _;

		// `frame_support`'s iteration docs warn that adding/removing values in the map "while
		// doing this" gives "undefined results". Our reading of the actual mechanism (`next_key`
		// is a live point-query against current storage, not a snapshot) says a fresh insertion
		// elsewhere in the map between two `update_agg_stats` calls is safe: it can't skip or
		// re-visit any entry that already existed when the pass started. This test proves that
		// empirically for the concrete scenario that matters here — a limit order filling for a
		// brand-new (Lp, Asset) pair (the eager-seed path) while a decay pass is genuinely
		// in-progress (a persisted, non-empty cursor) — rather than relying only on that reading.
		const NUM_LPS: u64 = 3;
		for lp in 0..NUM_LPS {
			LiquidityProvider::on_limit_order_filled(&lp, &Asset::Eth, 1_000_000_000u128); // 1000 usd
			LiquidityProvider::on_limit_order_filled(&lp, &Asset::Eth, 200_000_000u128); // 200 usd
		}
		assert_eq!(LpAggStats::<Test>::iter().count(), NUM_LPS as usize);

		let per_item_weight = <Test as crate::Config>::WeightInfo::update_agg_stats_item();
		const NEW_LP: u64 = 999;

		let mut cursor: Option<Vec<u8>> = None;
		let mut calls = 0u64;
		let mut new_entry_seeded_mid_pass = false;
		loop {
			calls += 1;
			// Generous bound: the mid-pass insertion may or may not add one more call depending
			// on where its key falls relative to the cursor. The property under test is
			// termination, not the exact count (already covered by the test above).
			assert!(calls <= NUM_LPS + 3, "pass should terminate in a bounded number of calls");
			LiquidityProvider::update_agg_stats(calls, cursor.clone(), per_item_weight);
			cursor = StatsUpdateCursor::<Test>::get();

			if calls == 1 {
				assert!(cursor.is_some(), "pass should still be genuinely in progress");
				// Simulate a fill landing in an intervening block, for a brand-new (Lp, Asset)
				// pair — this exercises the eager-seed path inserting a new LpAggStats entry
				// while the cursor above is persisted mid-pass.
				LiquidityProvider::on_limit_order_filled(&NEW_LP, &Asset::Eth, 700_000_000u128); // 700 usd
				assert!(LpAggStats::<Test>::get(NEW_LP, Asset::Eth).is_some());
				new_entry_seeded_mid_pass = true;
			}

			if cursor.is_none() {
				break;
			}
		}
		assert!(new_entry_seeded_mid_pass);

		// Every entry that existed when the pass started was still reached and correctly
		// decayed exactly once — the mid-pass insertion caused no skip.
		for lp in 0..NUM_LPS {
			assert_eq!(LpDeltaStats::<Test>::get(lp, Asset::Eth), None);
			let agg_stats = LpAggStats::<Test>::get(lp, Asset::Eth).unwrap();
			assert!(is_within_tiny_error(
				fixed_u128_to_f64(agg_stats.avg_limit_usd_volume.one_day),
				expected_ema(1000f64, 200f64, 1u32)
			));
		}

		// The mid-pass-inserted entry is intact either way: it was either swept into this same
		// pass (decayed once from its seed with a zero delta, since it has no pending delta of
		// its own) if its key fell ahead of the cursor, or left untouched, waiting for the next
		// pass, if its key fell behind. Both are correct outcomes — what matters is that it's
		// neither lost nor double-processed.
		let new_lp_stats = LpAggStats::<Test>::get(NEW_LP, Asset::Eth).unwrap();
		let seeded_value = 700f64;
		let decayed_once = expected_ema(seeded_value, 0f64, 1u32);
		let one_day = fixed_u128_to_f64(new_lp_stats.avg_limit_usd_volume.one_day);
		assert!(
			is_within_tiny_error(one_day, seeded_value) ||
				is_within_tiny_error(one_day, decayed_once),
			"new entry should be either untouched ({seeded_value}) or decayed exactly once \
			 ({decayed_once}), got {one_day}"
		);
	});
}

#[test]
fn on_idle_does_nothing_before_interval_elapses() {
	new_test_ext().execute_with(|| {
		use frame_support::{traits::Hooks, weights::Weight};

		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 1_000_000_000u128);
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 500_000_000u128);
		StatsLastUpdatedAt::<Test>::put(0u64);

		LiquidityProvider::on_idle(
			STATS_UPDATE_INTERVAL_IN_BLOCKS - 1,
			Weight::from_parts(u64::MAX, u64::MAX),
		);

		// Not due yet: the pending delta is untouched and no pass has started.
		assert!(LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth).is_some());
		assert!(StatsUpdateCursor::<Test>::get().is_none());
	});
}

#[test]
fn on_idle_runs_and_completes_the_pass_once_due() {
	new_test_ext().execute_with(|| {
		use frame_support::{traits::Hooks, weights::Weight};

		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 1_000_000_000u128);
		LiquidityProvider::on_limit_order_filled(&LP_ACCOUNT, &Asset::Eth, 500_000_000u128);
		StatsLastUpdatedAt::<Test>::put(0u64);

		LiquidityProvider::on_idle(
			STATS_UPDATE_INTERVAL_IN_BLOCKS,
			Weight::from_parts(u64::MAX, u64::MAX),
		);

		assert!(LpDeltaStats::<Test>::get(LP_ACCOUNT, Asset::Eth).is_none());
		assert!(StatsUpdateCursor::<Test>::get().is_none());
		assert_eq!(StatsLastUpdatedAt::<Test>::get(), STATS_UPDATE_INTERVAL_IN_BLOCKS);
	});
}

#[test]
fn test_purge_balances() {
	new_test_ext().execute_with(|| {
		const AMOUNT: AssetAmount = 1_000_000;

		MockBalanceApi::credit_account(&LP_ACCOUNT, Asset::Eth, AMOUNT);
		MockBalanceApi::credit_account(&LP_ACCOUNT_2, Asset::Flip, AMOUNT);

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			OriginTrait::signed(LP_ACCOUNT),
			EncodedAddress::Eth([0x01; 20])
		));

		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			OriginTrait::signed(LP_ACCOUNT_2),
			EncodedAddress::Eth([0x02; 20])
		));

		// Purge balances
		let accounts = vec![
			(LP_ACCOUNT, Asset::Eth, AMOUNT),
			(LP_ACCOUNT_2, Asset::Flip, AMOUNT),
			(NON_LP_ACCOUNT, Asset::Usdc, AMOUNT),
		];
		assert_ok!(Pallet::<Test>::purge_balances(RuntimeOrigin::root(), accounts));

		assert_events_match!(Test,
			RuntimeEvent::LiquidityProvider(Event::AssetBalancePurged {
			account_id: LP_ACCOUNT,
			asset: Asset::Eth,
			amount: AMOUNT,
			..
		}) => (),
			RuntimeEvent::LiquidityProvider(Event::AssetBalancePurged {
			account_id: LP_ACCOUNT_2,
			asset: Asset::Flip,
			amount: AMOUNT,
			..
		}) => (),
			RuntimeEvent::LiquidityProvider(Event::AssetBalancePurgeFailed {
			account_id: NON_LP_ACCOUNT,
			asset: Asset::Usdc,
			amount: AMOUNT,
			..
		}) => ()
		);

		assert!(MockBalanceApi::free_balances(&LP_ACCOUNT)
			.iter()
			.all(|(_, amount)| *amount == 0));

		assert!(MockBalanceApi::free_balances(&LP_ACCOUNT_2)
			.iter()
			.all(|(_, amount)| *amount == 0));
	});
}

mod withdrawal_restriction {
	use super::*;
	use cf_chains::AccountOrAddress;
	use cf_traits::mocks::withdrawal_address_restriction::MockWithdrawalAddressRestriction;
	use sp_runtime::DispatchError;

	fn not_allowed() -> DispatchError {
		DispatchError::Other("MockWithdrawalAddressRestriction: destination not allowed")
	}

	#[test]
	fn external_withdrawal_is_gated_by_the_whitelist() {
		new_test_ext().execute_with(|| {
			let allowed = [0xaa; 20];
			let disallowed = [0xbb; 20];

			MockBalanceApi::insert_balance(LP_ACCOUNT, Asset::Eth, 1_000);
			MockWithdrawalAddressRestriction::restrict_to(
				&LP_ACCOUNT,
				vec![AccountOrAddress::ExternalAddress(ForeignChainAddress::Eth(allowed.into()))],
			);

			// A different address is blocked...
			assert_noop!(
				LiquidityProvider::withdraw_asset(
					RuntimeOrigin::signed(LP_ACCOUNT),
					100,
					Asset::Eth,
					EncodedAddress::Eth(disallowed),
				),
				not_allowed()
			);
			// ...the whitelisted one succeeds.
			assert_ok!(LiquidityProvider::withdraw_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
				100,
				Asset::Eth,
				EncodedAddress::Eth(allowed),
			));
		});
	}

	#[test]
	fn internal_transfer_is_gated_by_the_whitelist() {
		new_test_ext().execute_with(|| {
			MockBalanceApi::insert_balance(LP_ACCOUNT, Asset::Eth, 1_000);
			assert_ok!(LiquidityProvider::register_liquidity_refund_address(
				RuntimeOrigin::signed(LP_ACCOUNT_2),
				EncodedAddress::Eth(Default::default()),
			));

			// Restrict to some other account: transferring to LP_ACCOUNT_2 is blocked.
			MockWithdrawalAddressRestriction::restrict_to(
				&LP_ACCOUNT,
				vec![AccountOrAddress::InternalAccount(NON_LP_ACCOUNT)],
			);
			assert_noop!(
				LiquidityProvider::transfer_asset(
					RuntimeOrigin::signed(LP_ACCOUNT),
					100,
					Asset::Eth,
					LP_ACCOUNT_2,
				),
				not_allowed()
			);

			// Allow LP_ACCOUNT_2: the transfer succeeds.
			MockWithdrawalAddressRestriction::restrict_to(
				&LP_ACCOUNT,
				vec![AccountOrAddress::InternalAccount(LP_ACCOUNT_2)],
			);
			assert_ok!(LiquidityProvider::transfer_asset(
				RuntimeOrigin::signed(LP_ACCOUNT),
				100,
				Asset::Eth,
				LP_ACCOUNT_2,
			));
		});
	}
}
