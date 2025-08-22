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

use super::*;

use crate::mocks::*;
use cf_test_utilities::{assert_event_sequence, assert_events_eq};
use cf_traits::{SafeMode, SetSafeMode};
use frame_support::{assert_noop, assert_ok, sp_runtime, traits::OriginTrait};

type AccountId = u64;

const TIER_5_BPS: BoostPoolTier = 5;
const TIER_10_BPS: BoostPoolTier = 10;
const TIER_30_BPS: BoostPoolTier = 30;

#[test]
fn test_fee_math() {
	use crate::utils::*;

	assert_eq!(fee_from_boosted_amount(1_000_000, 10), 1_000);

	assert_eq!(fee_from_provided_amount(1_000_000, 10), Ok(1_001));
}

const INIT_BOOSTER_ETH_BALANCE: AssetAmount = 1_000_000_000;
const INIT_BOOSTER_FLIP_BALANCE: AssetAmount = 1_000_000_000;

fn setup() {
	assert_ok!(LendingPools::create_boost_pools(
		RuntimeOrigin::root(),
		vec![
			BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS },
			BoostPoolId { asset: Asset::Eth, tier: TIER_10_BPS },
			BoostPoolId { asset: Asset::Eth, tier: TIER_30_BPS },
			BoostPoolId { asset: Asset::Flip, tier: TIER_5_BPS },
		]
	));

	<Test as crate::Config>::Balance::credit_account(
		&BOOSTER_1,
		Asset::Eth,
		INIT_BOOSTER_ETH_BALANCE,
	);

	<Test as crate::Config>::Balance::credit_account(
		&BOOSTER_1,
		Asset::Flip,
		INIT_BOOSTER_FLIP_BALANCE,
	);

	<Test as crate::Config>::Balance::credit_account(
		&BOOSTER_2,
		Asset::Eth,
		INIT_BOOSTER_ETH_BALANCE,
	);

	assert_eq!(get_lp_balance(&BOOSTER_1, Asset::Eth), INIT_BOOSTER_ETH_BALANCE);
}

fn get_lp_balance(lp: &AccountId, asset: Asset) -> AssetAmount {
	let balances = <Test as crate::Config>::Balance::free_balances(lp);
	balances[asset]
}

fn get_lp_eth_balance(lp: &AccountId) -> AssetAmount {
	get_lp_balance(lp, Asset::Eth)
}

fn get_available_amount_for_booster(
	asset: Asset,
	boost_tier: BoostPoolTier,
	booster: AccountId,
) -> Option<AssetAmount> {
	let core_pool_id = BoostPools::<Test>::get(asset, boost_tier).unwrap().core_pool_id;
	CorePools::<Test>::get(asset, core_pool_id)
		.unwrap()
		.get_available_amount_for_account(&booster)
}

#[track_caller]
fn assert_boosted(
	asset: Asset,
	expected_prewitnessed_deposit_id: PrewitnessedDepositId,
	expected_pools: impl IntoIterator<Item = BoostPoolTier>,
) {
	let boost_pools: Vec<_> = BoostedDeposits::<Test>::get(asset, expected_prewitnessed_deposit_id)
		.expect("deposit must be boosted")
		.keys()
		.copied()
		.collect();

	assert_eq!(boost_pools, Vec::from_iter(expected_pools.into_iter()));
}

#[track_caller]
fn assert_not_boosted(asset: Asset, expected_prewitnessed_deposit_id: PrewitnessedDepositId) {
	assert_eq!(BoostedDeposits::<Test>::get(asset, expected_prewitnessed_deposit_id), None)
}

#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_NETWORK_FEE_DEDUCTION: Percent = Percent::from_percent(50);

		// Check that the default values are different from the new ones
		assert_ne!(NetworkFeeDeductionFromBoostPercent::<Test>::get(), NEW_NETWORK_FEE_DEDUCTION);

		// Update all config items at the same time
		assert_ok!(LendingPools::update_pallet_config(
			RuntimeOrigin::root(),
			vec![PalletConfigUpdate::SetNetworkFeeDeductionFromBoost {
				deduction_percent: NEW_NETWORK_FEE_DEDUCTION
			}]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(NetworkFeeDeductionFromBoostPercent::<Test>::get(), NEW_NETWORK_FEE_DEDUCTION);

		// Check that the events were emitted
		assert_events_eq!(
			Test,
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: PalletConfigUpdate::SetNetworkFeeDeductionFromBoost {
					deduction_percent: NEW_NETWORK_FEE_DEDUCTION
				}
			}),
		);

		// Make sure that only governance can update the config
		assert_noop!(
			LendingPools::update_pallet_config(
				RuntimeOrigin::signed(LP),
				vec![].try_into().unwrap()
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn test_add_boost_funds() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;

		setup();

		// Should have all funds in the lp account and non in the pool yet.
		assert_eq!(get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1), None);
		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE);

		// Should not be able to add zero funds
		assert_noop!(
			LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				Asset::Eth,
				0,
				TIER_5_BPS
			),
			crate::Error::<Test>::AmountMustBeNonZero
		);

		// Add some of the LP funds to the boost pool
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			BOOST_FUNDS,
			TIER_5_BPS
		));

		// Should see some of the funds in the pool now and some funds missing from the LP account
		assert_eq!(
			get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1),
			Some(BOOST_FUNDS)
		);
		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE - BOOST_FUNDS);

		System::assert_last_event(RuntimeEvent::LendingPools(Event::BoostFundsAdded {
			booster_id: BOOSTER_1,
			boost_pool: BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS },
			amount: BOOST_FUNDS,
		}));
	});
}

#[test]
fn basic_boosting() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);

		setup();

		NetworkFeeDeductionFromBoostPercent::<Test>::set(Percent::from_percent(50));

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			BOOST_FUNDS,
			TIER_5_BPS
		));

		assert_ok!(LendingPools::try_boosting(DEPOSIT_ID, Asset::Eth, DEPOSIT_AMOUNT, TIER_5_BPS));

		assert_boosted(Asset::Eth, DEPOSIT_ID, [TIER_5_BPS]);

		const TOTAL_BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * TIER_5_BPS as u128 / 10_000;
		const NETWORK_FEE: AssetAmount = TOTAL_BOOST_FEE / 2;

		assert_eq!(
			LendingPools::finalise_boost(DEPOSIT_ID, Asset::Eth),
			BoostFinalisationOutcome { network_fee: NETWORK_FEE }
		);

		assert_not_boosted(Asset::Eth, DEPOSIT_ID);

		assert_eq!(
			get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1),
			Some(BOOST_FUNDS + TOTAL_BOOST_FEE - NETWORK_FEE)
		);

		// Check that finalising boost also finalises the loan:
		for pool in CorePools::<Test>::iter_values() {
			assert!(pool.pending_loans.is_empty());
		}
	});
}

#[test]
fn boosted_deposit_is_lost() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);

		setup();

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			BOOST_FUNDS,
			TIER_5_BPS
		));

		assert_ok!(LendingPools::try_boosting(DEPOSIT_ID, Asset::Eth, DEPOSIT_AMOUNT, TIER_5_BPS));

		assert_boosted(Asset::Eth, DEPOSIT_ID, [TIER_5_BPS]);

		LendingPools::process_deposit_as_lost(DEPOSIT_ID, Asset::Eth);

		assert_not_boosted(Asset::Eth, DEPOSIT_ID);

		const TOTAL_BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * TIER_5_BPS as u128 / 10_000;

		assert_eq!(
			get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1),
			Some(BOOST_FUNDS - DEPOSIT_AMOUNT + TOTAL_BOOST_FEE)
		);
	});
}

#[test]
fn stop_boosting() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT_1: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);

		setup();

		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE);

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			BOOSTER_AMOUNT_1,
			TIER_5_BPS
		));

		assert_ok!(LendingPools::try_boosting(DEPOSIT_ID, Asset::Eth, DEPOSIT_AMOUNT, TIER_30_BPS));

		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1);

		// Booster stops boosting and get the available portion of their funds immediately:
		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			TIER_5_BPS
		));

		const BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * TIER_5_BPS as u128 / 10_000;
		const AVAILABLE_BOOST_AMOUNT: AssetAmount = BOOSTER_AMOUNT_1 - (DEPOSIT_AMOUNT - BOOST_FEE);
		assert_eq!(
			get_lp_eth_balance(&BOOSTER_1),
			INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1 + AVAILABLE_BOOST_AMOUNT
		);

		System::assert_last_event(RuntimeEvent::LendingPools(Event::StoppedBoosting {
			booster_id: BOOSTER_1,
			boost_pool: BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS },
			unlocked_amount: AVAILABLE_BOOST_AMOUNT,
			pending_boosts: BTreeSet::from_iter(vec![DEPOSIT_ID]),
		}));

		// Deposit is finalised, the booster gets their remaining funds from the pool:
		LendingPools::finalise_boost(DEPOSIT_ID, Asset::Eth);
		assert_eq!(get_lp_eth_balance(&BOOSTER_1), INIT_BOOSTER_ETH_BALANCE + BOOST_FEE);
	});
}

#[test]
fn skip_zero_amount_pool() {
	// 10 bps has 0 available funds, but we are able to skip it and
	// boost with the next tier pool

	const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
	new_test_ext().execute_with(|| {
		const POOL_AMOUNT: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 1_000_000_000;

		setup();

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			POOL_AMOUNT,
			TIER_5_BPS
		));

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			Asset::Eth,
			POOL_AMOUNT,
			TIER_30_BPS
		));

		assert_ok!(LendingPools::try_boosting(DEPOSIT_ID, Asset::Eth, DEPOSIT_AMOUNT, TIER_30_BPS));

		// Should be able to boost without the 10bps pool:
		assert_boosted(Asset::Eth, DEPOSIT_ID, [TIER_5_BPS, TIER_30_BPS]);
	});
}

#[test]
fn add_boost_funds_is_disabled_by_safe_mode() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;

		setup();

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
			add_boost_funds_enabled: false,
			..PalletSafeMode::CODE_GREEN
		});

		// Should not be able to add funds to the boost pool
		assert_noop!(
			LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				Asset::Eth,
				BOOST_FUNDS,
				TIER_5_BPS
			),
			crate::Error::<Test>::AddBoostFundsDisabled
		);

		assert_eq!(get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1), None);

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::CODE_GREEN);

		// Should be able to add funds to the boost pool now that the safe mode is turned off
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			BOOST_FUNDS,
			TIER_5_BPS
		));
		assert_eq!(
			get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1),
			Some(BOOST_FUNDS)
		);
	});
}

#[test]
fn stop_boosting_is_disabled_by_safe_mode() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;

		setup();

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			BOOST_FUNDS,
			TIER_5_BPS
		));

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
			stop_boosting_enabled: false,
			..PalletSafeMode::CODE_GREEN
		});

		// Should not be able to stop boosting
		assert_noop!(
			LendingPools::stop_boosting(RuntimeOrigin::signed(BOOSTER_1), Asset::Eth, TIER_5_BPS),
			crate::Error::<Test>::StopBoostingDisabled
		);

		assert_eq!(
			get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1),
			Some(BOOST_FUNDS)
		);

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::CODE_GREEN);

		// Should be able to stop boosting now that the safe mode is turned off
		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			TIER_5_BPS
		));
		assert_eq!(get_available_amount_for_booster(Asset::Eth, TIER_5_BPS, BOOSTER_1), None);
	});
}

#[test]
fn test_create_boost_pools() {
	new_test_ext().execute_with(|| {
		// Make sure the pools do not exists already
		assert!(BoostPools::<Test>::get(Asset::Eth, TIER_5_BPS).is_none());
		assert!(BoostPools::<Test>::get(Asset::Eth, TIER_10_BPS).is_none());
		assert!(BoostPools::<Test>::get(Asset::Flip, TIER_5_BPS).is_none());

		// Create all 3 pools in one go
		assert_ok!(Pallet::<Test>::create_boost_pools(
			RuntimeOrigin::root(),
			vec![
				BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS },
				BoostPoolId { asset: Asset::Eth, tier: TIER_10_BPS },
				BoostPoolId { asset: Asset::Flip, tier: TIER_5_BPS },
			]
		));

		// // Check they now exist
		assert!(BoostPools::<Test>::get(Asset::Eth, TIER_5_BPS).is_some());
		assert!(BoostPools::<Test>::get(Asset::Eth, TIER_10_BPS).is_some());
		assert!(BoostPools::<Test>::get(Asset::Flip, TIER_5_BPS).is_some());

		// Check that all 3 emitted the creation event
		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::BoostPoolCreated {
				boost_pool: BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS },
			}),
			RuntimeEvent::LendingPools(Event::BoostPoolCreated {
				boost_pool: BoostPoolId { asset: Asset::Eth, tier: TIER_10_BPS },
			}),
			RuntimeEvent::LendingPools(Event::BoostPoolCreated {
				boost_pool: BoostPoolId { asset: Asset::Flip, tier: TIER_5_BPS },
			})
		);

		// Should not be able to create the same pool again
		assert_noop!(
			Pallet::<Test>::create_boost_pools(
				RuntimeOrigin::root(),
				vec![BoostPoolId { asset: Asset::Eth, tier: TIER_5_BPS }]
			),
			crate::Error::<Test>::PoolAlreadyExists
		);

		// Make sure it did not remove the existing boost pool
		assert!(BoostPools::<Test>::get(Asset::Eth, TIER_5_BPS).is_some());

		// Should not be able to create a pool with a tier of 0
		assert_noop!(
			Pallet::<Test>::create_boost_pools(
				RuntimeOrigin::root(),
				vec![BoostPoolId { asset: Asset::Eth, tier: 0 }]
			),
			crate::Error::<Test>::InvalidBoostPoolTier
		);

		// Make sure that only governance can create boost pools
		assert_noop!(
			Pallet::<Test>::create_boost_pools(OriginTrait::none(), vec![]),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn boost_account_balance() {
	new_test_ext().execute_with(|| {
		setup();

		const ETH_AMOUNT_1: AssetAmount = 50_000;
		const ETH_AMOUNT_2: AssetAmount = 25_000;
		const FLIP_AMOUNT: AssetAmount = 5_000;
		const BOOSTED_AMOUNT: AssetAmount = 20_000;

		// Add funds to two different pools:
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			ETH_AMOUNT_1,
			TIER_5_BPS
		));

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			ETH_AMOUNT_2,
			TIER_10_BPS
		));

		// Add funds in a different asset to check that we
		// can distinguish between different assets:
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Flip,
			FLIP_AMOUNT,
			TIER_5_BPS
		));

		// Add a different booster to make sure that their funds
		// don't affect the result for BOOSTER_1:
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			Asset::Eth,
			ETH_AMOUNT_1,
			TIER_10_BPS
		));

		// A portion of the funds will is pending due to an unfinalised boost
		assert_ok!(LendingPools::try_boosting(
			PrewitnessedDepositId(0),
			Asset::Eth,
			BOOSTED_AMOUNT,
			TIER_30_BPS
		));

		let boost_fee = BOOSTED_AMOUNT * TIER_5_BPS as u128 / 10_000;

		use cf_traits::BoostBalancesApi;

		// Check that we collect funds from all pools and include funds from unfinalised boosts,
		// ignoring other accounts and assets:
		assert_eq!(
			LendingPools::boost_pool_account_balance(&BOOSTER_1, Asset::Eth),
			ETH_AMOUNT_1 + ETH_AMOUNT_2 + boost_fee
		);
	});
}

#[test]
fn boost_pool_details() {
	new_test_ext().execute_with(|| {
		setup();
		const ETH_AMOUNT_1: AssetAmount = 50_000;
		const ETH_AMOUNT_2: AssetAmount = 25_000;
		const BOOSTED_AMOUNT: AssetAmount = 30_000;

		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(3);

		const NETWORK_FEE_DEDUCTION: Percent = Percent::from_percent(50);

		NetworkFeeDeductionFromBoostPercent::<Test>::set(NETWORK_FEE_DEDUCTION);

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Eth,
			ETH_AMOUNT_1,
			TIER_10_BPS
		));

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			Asset::Eth,
			ETH_AMOUNT_2,
			TIER_10_BPS
		));

		assert_ok!(LendingPools::try_boosting(DEPOSIT_ID, Asset::Eth, BOOSTED_AMOUNT, TIER_10_BPS));

		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_2),
			Asset::Eth,
			TIER_10_BPS
		));

		assert_eq!(
			get_boost_pool_details::<Test>(Asset::Eth).get(&TIER_10_BPS).cloned().unwrap(),
			BoostPoolDetails {
				available_amounts: BTreeMap::from_iter([(BOOSTER_1, 30_020)]),
				pending_boosts: BTreeMap::from_iter([(
					DEPOSIT_ID,
					BTreeMap::from_iter([
						// Note the network fee deduction:
						(BOOSTER_1, OwedAmount { total: 20_000 - 10, fee: 10 }),
						(BOOSTER_2, OwedAmount { total: 10_000 - 5, fee: 5 })
					])
				)]),
				pending_withdrawals: BTreeMap::from_iter([(
					BOOSTER_2,
					BTreeSet::from_iter([DEPOSIT_ID])
				)]),
				network_fee_deduction_percent: NETWORK_FEE_DEDUCTION
			}
		);
	});
}
