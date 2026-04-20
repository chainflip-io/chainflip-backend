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

use crate::{
	boost::{BoostPoolContribution, BoostedDeposit, BOOST_FEE},
	core_lending_pool::CoreLoanId,
	general_lending::{GeneralLoan, LoanType},
	mocks::*,
};
use cf_test_utilities::{assert_event_sequence, assert_events_eq};
use cf_traits::{
	mocks::{balance_api::MockBalance, price_feed_api::MockPriceFeedApi},
	DeregistrationCheck, SafeMode, SetSafeMode,
};
use frame_support::{
	assert_noop, assert_ok,
	sp_runtime::{self, bounded_vec},
};
use sp_runtime::Permill;

type AccountId = u64;

const BOOST_FEE_BPS: BoostPoolTier = 5;
const BOOST_ASSET: Asset = Asset::Eth;

const INIT_BOOSTER_ETH_BALANCE: AssetAmount = 1_000_000_000;
const INIT_BOOSTER_FLIP_BALANCE: AssetAmount = 1_000_000_000;

fn setup_lending_pool_for_boost() {
	assert_ok!(LendingPools::create_lending_pool(RuntimeOrigin::root(), BOOST_ASSET));
	assert_ok!(LendingPools::update_whitelist(RuntimeOrigin::root(), WhitelistUpdate::SetAllowAll));
	MockPriceFeedApi::set_price_usd_fine(BOOST_ASSET, 1_000_000);

	BoostConfig::<Test>::set(BoostConfiguration {
		network_fee_deduction_from_boost_percent: Percent::from_percent(0),
		minimum_add_funds_amount: BTreeMap::default(),
		min_lending_pool_share: Percent::from_percent(30),
	});

	MockBalance::credit_account(&BOOSTER_1, BOOST_ASSET, INIT_BOOSTER_ETH_BALANCE);

	MockBalance::credit_account(&BOOSTER_2, BOOST_ASSET, INIT_BOOSTER_ETH_BALANCE);

	assert_eq!(MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET), INIT_BOOSTER_ETH_BALANCE);
}

fn setup_legacy_boost_pools() {
	assert_ok!(LendingPools::create_boost_pools(
		RuntimeOrigin::root(),
		vec![
			BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS },
			BoostPoolId { asset: Asset::Flip, tier: BOOST_FEE_BPS },
		]
	));

	BoostConfig::<Test>::set(BoostConfiguration {
		network_fee_deduction_from_boost_percent: Percent::from_percent(0),
		minimum_add_funds_amount: BTreeMap::default(),
		min_lending_pool_share: Percent::from_percent(30),
	});

	<Test as crate::Config>::Balance::credit_account(
		&BOOSTER_1,
		BOOST_ASSET,
		INIT_BOOSTER_ETH_BALANCE,
	);

	<Test as crate::Config>::Balance::credit_account(
		&BOOSTER_1,
		Asset::Flip,
		INIT_BOOSTER_FLIP_BALANCE,
	);

	<Test as crate::Config>::Balance::credit_account(
		&BOOSTER_2,
		BOOST_ASSET,
		INIT_BOOSTER_ETH_BALANCE,
	);

	assert_eq!(MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET), INIT_BOOSTER_ETH_BALANCE);
}

fn get_available_amount_for_booster(asset: Asset, booster: AccountId) -> Option<AssetAmount> {
	let core_pool_id = BoostPools::<Test>::get(asset, BOOST_FEE).unwrap().core_pool_id;
	CorePools::<Test>::get(asset, core_pool_id)
		.unwrap()
		.get_available_amount_for_account(&booster)
}

fn get_supply_position(asset: Asset, lender: AccountId) -> Option<AssetAmount> {
	GeneralLendingPools::<Test>::get(asset)
		.and_then(|pool| pool.get_supply_position_for_account(&lender).ok())
}

#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_NETWORK_FEE_DEDUCTION: Percent = Percent::from_percent(50);

		let new_boost_config = BoostConfiguration {
			network_fee_deduction_from_boost_percent: NEW_NETWORK_FEE_DEDUCTION,
			minimum_add_funds_amount: BTreeMap::from([(Asset::Btc, 15000_u128)]),
			min_lending_pool_share: Percent::from_percent(50),
		};

		const NEW_LENDING_POOL_CONFIG: LendingPoolConfiguration = LendingPoolConfiguration {
			origination_fee: Permill::from_percent(1),
			liquidation_fee: Permill::from_percent(2),
			interest_rate_curve: InterestRateConfiguration {
				interest_at_zero_utilisation: Permill::from_percent(1),
				junction_utilisation: Permill::from_percent(41),
				interest_at_junction_utilisation: Permill::from_percent(6),
				interest_at_max_utilisation: Permill::from_percent(91),
			},
		};

		const NEW_LTV_THRESHOLDS: LtvThresholds = LtvThresholds {
			target: Permill::from_percent(61),
			topup: Some(Permill::from_percent(71)),
			soft_liquidation: Permill::from_percent(81),
			soft_liquidation_abort: Permill::from_percent(80),
			hard_liquidation: Permill::from_percent(91),
			hard_liquidation_abort: Permill::from_percent(90),
			low_ltv: Permill::from_percent(60),
		};

		const NEW_NETWORK_FEE_CONTRIBUTIONS_FROM_LENDING: NetworkFeeContributions =
			NetworkFeeContributions {
				extra_interest: Permill::from_percent(1),
				from_origination_fee: Permill::from_percent(2),
				from_liquidation_fee: Permill::from_percent(3),
				low_ltv_penalty_max: Permill::from_percent(5),
			};

		const NEW_FEE_SWAP_INTERVAL_BLOCKS: u32 = 700;
		const NEW_INTEREST_PAYMENT_INTERVAL_BLOCKS: u32 = 712;
		const NEW_FEE_SWAP_THRESHOLD_USD: AssetAmount = 42;
		const NEW_ORACLE_SLIPPAGE_SOFT_LIQUIDATION: BasisPoints = 43;
		const NEW_ORACLE_SLIPPAGE_HARD_LIQUIDATION: BasisPoints = 44;
		const NEW_ORACLE_SLIPPAGE_FEE_SWAP: BasisPoints = 45;
		const NEW_INTEREST_COLLECTION_THRESHOLD_USD: AssetAmount = 46;

		const NEW_SOFT_LIQUIDATION_SWAP_CHUNK_SIZE_USD: AssetAmount = 30_000_000_000;
		const NEW_HARD_LIQUIDATION_SWAP_CHUNK_SIZE_USD: AssetAmount = 45_000_000_000;
		const NEW_MINIMUM_LOAN_AMOUNT_USD: AssetAmount = 12345;
		const NEW_MINIMUM_UPDATE_LOAN_AMOUNT_USD: AssetAmount = 1234;
		const NEW_MINIMUM_UPDATE_COLLATERAL_AMOUNT_USD: AssetAmount = 567;
		const NEW_MINIMUM_SUPPLY_AMOUNT_USD: AssetAmount = 7783;

		let update_boost_config: PalletConfigUpdate =
			PalletConfigUpdate::SetBoostConfig { config: new_boost_config.clone() };

		const UPDATE_LENDING_POOL_CONFIG: PalletConfigUpdate =
			PalletConfigUpdate::SetLendingPoolConfiguration {
				asset: None,
				config: Some(NEW_LENDING_POOL_CONFIG),
			};

		const UPDATE_LTV_THRESHOLDS: PalletConfigUpdate =
			PalletConfigUpdate::SetLtvThresholds { ltv_thresholds: NEW_LTV_THRESHOLDS };

		const UPDATE_NETWORK_FEE_CONTRIBUTIONS: PalletConfigUpdate =
			PalletConfigUpdate::SetNetworkFeeContributions {
				contributions: NEW_NETWORK_FEE_CONTRIBUTIONS_FROM_LENDING,
			};

		const UPDATE_FEE_SWAP_INTERVAL_BLOCKS: PalletConfigUpdate =
			PalletConfigUpdate::SetFeeSwapIntervalBlocks(NEW_FEE_SWAP_INTERVAL_BLOCKS);

		const UPDATE_INTEREST_PAYMENT_INTERVAL_BLOCKS: PalletConfigUpdate =
			PalletConfigUpdate::SetInterestPaymentIntervalBlocks(
				NEW_INTEREST_PAYMENT_INTERVAL_BLOCKS,
			);

		const UPDATE_FEE_SWAP_THRESHOLD_USD: PalletConfigUpdate =
			PalletConfigUpdate::SetFeeSwapThresholdUsd(NEW_FEE_SWAP_THRESHOLD_USD);

		const UPDATE_INTEREST_COLLECTION_THRESHOLD_USD: PalletConfigUpdate =
			PalletConfigUpdate::SetInterestCollectionThresholdUsd(
				NEW_INTEREST_COLLECTION_THRESHOLD_USD,
			);

		const UPDATE_ORACLE_SLIPPAGE_FOR_SWAPS: PalletConfigUpdate =
			PalletConfigUpdate::SetOracleSlippageForSwaps {
				soft_liquidation: NEW_ORACLE_SLIPPAGE_SOFT_LIQUIDATION,
				hard_liquidation: NEW_ORACLE_SLIPPAGE_HARD_LIQUIDATION,
				fee_swap: NEW_ORACLE_SLIPPAGE_FEE_SWAP,
			};

		const UPDATE_LIQUIDATION_SWAP_CHUNK_SIZE_USD: PalletConfigUpdate =
			PalletConfigUpdate::SetLiquidationSwapChunkSizeUsd {
				soft: NEW_SOFT_LIQUIDATION_SWAP_CHUNK_SIZE_USD,
				hard: NEW_HARD_LIQUIDATION_SWAP_CHUNK_SIZE_USD,
			};

		const UPDATE_LOAN_MINIMUMS: PalletConfigUpdate = PalletConfigUpdate::SetMinimumAmounts {
			minimum_loan_amount_usd: NEW_MINIMUM_LOAN_AMOUNT_USD,
			minimum_update_loan_amount_usd: NEW_MINIMUM_UPDATE_LOAN_AMOUNT_USD,
			minimum_update_collateral_amount_usd: NEW_MINIMUM_UPDATE_COLLATERAL_AMOUNT_USD,
			minimum_supply_amount_usd: NEW_MINIMUM_SUPPLY_AMOUNT_USD,
		};

		// Check that the default values are different from the new ones
		assert_ne!(BoostConfig::<Test>::get(), new_boost_config);
		assert_ne!(
			LendingConfig::<Test>::get().fee_swap_interval_blocks,
			NEW_FEE_SWAP_INTERVAL_BLOCKS
		);
		assert_ne!(
			LendingConfig::<Test>::get().interest_payment_interval_blocks,
			NEW_INTEREST_PAYMENT_INTERVAL_BLOCKS
		);
		assert_ne!(LendingConfig::<Test>::get().fee_swap_threshold_usd, NEW_FEE_SWAP_THRESHOLD_USD);
		assert_ne!(
			LendingConfig::<Test>::get().interest_collection_threshold_usd,
			NEW_INTEREST_COLLECTION_THRESHOLD_USD
		);
		assert_ne!(
			LendingConfig::<Test>::get().soft_liquidation_max_oracle_slippage,
			NEW_ORACLE_SLIPPAGE_SOFT_LIQUIDATION
		);
		assert_ne!(
			LendingConfig::<Test>::get().hard_liquidation_max_oracle_slippage,
			NEW_ORACLE_SLIPPAGE_HARD_LIQUIDATION
		);
		assert_ne!(
			LendingConfig::<Test>::get().fee_swap_max_oracle_slippage,
			NEW_ORACLE_SLIPPAGE_FEE_SWAP
		);
		assert_ne!(
			LendingConfig::<Test>::get().soft_liquidation_swap_chunk_size_usd,
			NEW_SOFT_LIQUIDATION_SWAP_CHUNK_SIZE_USD
		);
		assert_ne!(
			LendingConfig::<Test>::get().hard_liquidation_swap_chunk_size_usd,
			NEW_HARD_LIQUIDATION_SWAP_CHUNK_SIZE_USD
		);
		assert_ne!(
			LendingConfig::<Test>::get().minimum_loan_amount_usd,
			NEW_MINIMUM_LOAN_AMOUNT_USD
		);
		assert_ne!(
			LendingConfig::<Test>::get().minimum_update_loan_amount_usd,
			NEW_MINIMUM_UPDATE_LOAN_AMOUNT_USD
		);
		assert_ne!(
			LendingConfig::<Test>::get().minimum_update_collateral_amount_usd,
			NEW_MINIMUM_UPDATE_COLLATERAL_AMOUNT_USD
		);

		// Update all config items at the same time
		assert_ok!(LendingPools::update_pallet_config(
			RuntimeOrigin::root(),
			vec![
				update_boost_config.clone(),
				UPDATE_LENDING_POOL_CONFIG,
				UPDATE_LTV_THRESHOLDS,
				UPDATE_NETWORK_FEE_CONTRIBUTIONS,
				UPDATE_FEE_SWAP_INTERVAL_BLOCKS,
				UPDATE_INTEREST_PAYMENT_INTERVAL_BLOCKS,
				UPDATE_FEE_SWAP_THRESHOLD_USD,
				UPDATE_ORACLE_SLIPPAGE_FOR_SWAPS,
				UPDATE_LIQUIDATION_SWAP_CHUNK_SIZE_USD,
				UPDATE_INTEREST_COLLECTION_THRESHOLD_USD,
				UPDATE_LOAN_MINIMUMS,
			]
			.try_into()
			.unwrap()
		));

		// Check that the new values were set
		assert_eq!(BoostConfig::<Test>::get(), new_boost_config);

		assert_eq!(
			LendingConfig::<Test>::get(),
			LendingConfiguration {
				default_pool_config: NEW_LENDING_POOL_CONFIG,
				ltv_thresholds: NEW_LTV_THRESHOLDS,
				network_fee_contributions: NEW_NETWORK_FEE_CONTRIBUTIONS_FROM_LENDING,
				fee_swap_interval_blocks: NEW_FEE_SWAP_INTERVAL_BLOCKS,
				interest_payment_interval_blocks: NEW_INTEREST_PAYMENT_INTERVAL_BLOCKS,
				fee_swap_threshold_usd: NEW_FEE_SWAP_THRESHOLD_USD,
				interest_collection_threshold_usd: NEW_INTEREST_COLLECTION_THRESHOLD_USD,
				soft_liquidation_max_oracle_slippage: NEW_ORACLE_SLIPPAGE_SOFT_LIQUIDATION,
				hard_liquidation_max_oracle_slippage: NEW_ORACLE_SLIPPAGE_HARD_LIQUIDATION,
				soft_liquidation_swap_chunk_size_usd: NEW_SOFT_LIQUIDATION_SWAP_CHUNK_SIZE_USD,
				hard_liquidation_swap_chunk_size_usd: NEW_HARD_LIQUIDATION_SWAP_CHUNK_SIZE_USD,
				fee_swap_max_oracle_slippage: NEW_ORACLE_SLIPPAGE_FEE_SWAP,
				minimum_loan_amount_usd: NEW_MINIMUM_LOAN_AMOUNT_USD,
				minimum_update_loan_amount_usd: NEW_MINIMUM_UPDATE_LOAN_AMOUNT_USD,
				minimum_update_collateral_amount_usd: NEW_MINIMUM_UPDATE_COLLATERAL_AMOUNT_USD,
				minimum_supply_amount_usd: NEW_MINIMUM_SUPPLY_AMOUNT_USD,
				pool_config_overrides: BTreeMap::default(),
			}
		);

		// Check that the events were emitted
		assert_events_eq!(
			Test,
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated { update: update_boost_config }),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_LENDING_POOL_CONFIG
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_LTV_THRESHOLDS
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_NETWORK_FEE_CONTRIBUTIONS
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_FEE_SWAP_INTERVAL_BLOCKS
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_INTEREST_PAYMENT_INTERVAL_BLOCKS
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_FEE_SWAP_THRESHOLD_USD
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_ORACLE_SLIPPAGE_FOR_SWAPS
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated {
				update: UPDATE_LIQUIDATION_SWAP_CHUNK_SIZE_USD
			}),
			RuntimeEvent::LendingPools(Event::PalletConfigUpdated { update: UPDATE_LOAN_MINIMUMS }),
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
fn can_update_config_for_specific_asset() {
	// In this test we set one (default) config for all assets and create an override for BTC.
	// We will also test that we can remove the override.

	const NEW_LENDING_POOL_CONFIG: LendingPoolConfiguration = LendingPoolConfiguration {
		origination_fee: Permill::from_percent(1),
		liquidation_fee: Permill::from_percent(2),
		interest_rate_curve: InterestRateConfiguration {
			interest_at_zero_utilisation: Permill::from_percent(1),
			junction_utilisation: Permill::from_percent(41),
			interest_at_junction_utilisation: Permill::from_percent(6),
			interest_at_max_utilisation: Permill::from_percent(91),
		},
	};

	const NEW_LENDING_POOL_CONFIG_FOR_BTC: LendingPoolConfiguration = LendingPoolConfiguration {
		origination_fee: Permill::from_percent(2),
		liquidation_fee: Permill::from_percent(3),
		interest_rate_curve: InterestRateConfiguration {
			interest_at_zero_utilisation: Permill::from_percent(2),
			junction_utilisation: Permill::from_percent(42),
			interest_at_junction_utilisation: Permill::from_percent(7),
			interest_at_max_utilisation: Permill::from_percent(92),
		},
	};

	new_test_ext().execute_with(|| {
		// Executing in separate calls to make sure we don't rely on the order
		// of updates listed in the vector:
		assert_ok!(LendingPools::update_pallet_config(
			RuntimeOrigin::root(),
			bounded_vec![PalletConfigUpdate::SetLendingPoolConfiguration {
				asset: Some(Asset::Btc),
				config: Some(NEW_LENDING_POOL_CONFIG_FOR_BTC),
			}]
		));

		// This should not affect the config for BTC:
		assert_ok!(LendingPools::update_pallet_config(
			RuntimeOrigin::root(),
			bounded_vec![PalletConfigUpdate::SetLendingPoolConfiguration {
				asset: None,
				config: Some(NEW_LENDING_POOL_CONFIG),
			}]
		));

		assert_eq!(LendingConfig::<Test>::get().default_pool_config, NEW_LENDING_POOL_CONFIG);
		assert_eq!(
			LendingConfig::<Test>::get().pool_config_overrides,
			BTreeMap::from([(Asset::Btc, NEW_LENDING_POOL_CONFIG_FOR_BTC)])
		);

		assert_eq!(
			LendingConfig::<Test>::get().get_config_for_asset(BOOST_ASSET),
			&NEW_LENDING_POOL_CONFIG
		);

		assert_eq!(
			LendingConfig::<Test>::get().get_config_for_asset(Asset::Btc),
			&NEW_LENDING_POOL_CONFIG_FOR_BTC
		);

		// This should remove the override for BTC
		assert_ok!(LendingPools::update_pallet_config(
			RuntimeOrigin::root(),
			bounded_vec![PalletConfigUpdate::SetLendingPoolConfiguration {
				asset: Some(Asset::Btc),
				config: None,
			}]
		));

		assert_eq!(LendingConfig::<Test>::get().default_pool_config, NEW_LENDING_POOL_CONFIG);
		assert_eq!(LendingConfig::<Test>::get().pool_config_overrides, Default::default());
	});
}

#[test]
fn test_add_funds_to_legacy_boost_pool() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;

		setup_legacy_boost_pools();

		// Should have all funds in the lp account and non in the pool yet.
		assert_eq!(get_available_amount_for_booster(BOOST_ASSET, BOOSTER_1), None);
		assert_eq!(MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET), INIT_BOOSTER_ETH_BALANCE);

		// Should not be able to add zero funds
		assert_noop!(
			LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				0,
				BOOST_FEE_BPS
			),
			crate::Error::<Test>::AmountBelowMinimum
		);

		// Add some of the LP funds to the boost pool
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FUNDS,
			BOOST_FEE_BPS
		));

		// Should see some of the funds in the pool now and some funds missing from the LP account
		assert_eq!(get_available_amount_for_booster(BOOST_ASSET, BOOSTER_1), Some(BOOST_FUNDS));
		assert_eq!(
			MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET),
			INIT_BOOSTER_ETH_BALANCE - BOOST_FUNDS
		);

		System::assert_last_event(RuntimeEvent::LendingPools(Event::BoostFundsAdded {
			booster_id: BOOSTER_1,
			boost_pool: BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS },
			amount: BOOST_FUNDS,
		}));
	});
}

/// Basic boosting: first a deposit is boosted, then it is finalised. We check that
/// the pool earns some fees and that the events are emitted as expected.
#[test]
fn basic_boosting() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
		const LOAN_ID: LoanId = LoanId(0);

		setup_lending_pool_for_boost();

		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: Percent::from_percent(50),
			minimum_add_funds_amount: Default::default(),
			min_lending_pool_share: Percent::from_percent(30),
		});

		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FUNDS,
		));

		frame_system::Pallet::<Test>::reset_events();

		assert_ok!(LendingPools::try_boosting(
			DEPOSIT_ID,
			BOOST_ASSET,
			DEPOSIT_AMOUNT,
			BOOST_FEE_BPS
		));

		const TOTAL_BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000;
		const NETWORK_FEE: AssetAmount = TOTAL_BOOST_FEE / 2;
		const POOL_FEE: AssetAmount = TOTAL_BOOST_FEE - NETWORK_FEE;
		const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_BOOST_FEE;

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
				loan_id: LOAN_ID,
				loan_type: LoanType::Boost(DEPOSIT_ID),
				asset: BOOST_ASSET,
				principal_amount: REQUIRED_AMOUNT,
			}),
			RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
				loan_id: LOAN_ID,
				pool_fee: POOL_FEE,
				network_fee: NETWORK_FEE,
				broker_fee: 0,
			}),
		);

		assert_eq!(
			BoostLoans::<Test>::get(LOAN_ID),
			Some(GeneralLoan {
				id: LOAN_ID,
				asset: BOOST_ASSET,
				created_at_block: 1,
				owed_principal: DEPOSIT_AMOUNT,
				pending_interest: Default::default(),
			})
		);

		assert_eq!(
			BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID),
			Some(BoostedDeposit {
				deposit_amount: DEPOSIT_AMOUNT,
				lending_loan_id: Some(LOAN_ID),
				boost_pool_contribution: None,
			})
		);

		frame_system::Pallet::<Test>::reset_events();

		// Network fee was paid at loan creation, so finalise_boost returns no additional
		// network fee.
		LendingPools::finalise_boost(DEPOSIT_ID, BOOST_ASSET);

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
				loan_id: LOAN_ID,
				amount: DEPOSIT_AMOUNT,
				action_type: LoanRepaidActionType::BoostFinalisation,
			}),
			RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: false,
			}),
		);

		assert_eq!(BoostLoans::<Test>::get(LOAN_ID), None);
		assert_eq!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID), None);

		assert_eq!(get_supply_position(BOOST_ASSET, BOOSTER_1), Some(BOOST_FUNDS + POOL_FEE));
	});
}

/// Boosting does not depend on oracle prices for the deposit asset (no LTV check is
/// required to open a boost loan), so a stale price must not block the boost.
#[test]
fn boost_succeeds_with_stale_oracle_price() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
		const LOAN_ID: LoanId = LoanId(0);

		setup_lending_pool_for_boost();

		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FUNDS,
		));

		MockPriceFeedApi::set_stale(BOOST_ASSET, true);

		assert_ok!(LendingPools::try_boosting(
			DEPOSIT_ID,
			BOOST_ASSET,
			DEPOSIT_AMOUNT,
			BOOST_FEE_BPS
		));

		assert!(BoostLoans::<Test>::get(LOAN_ID).is_some());
		assert!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID).is_some());
	});
}

/// Uses BTC lending pool only (no legacy boost pool)
#[test]
fn boosted_deposit_is_lost() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
		const LOAN_ID: LoanId = LoanId(0);
		const NETWORK_FEE_PCT: u128 = 50;

		setup_lending_pool_for_boost();

		// Override network fee to 50%
		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: Percent::from_percent(NETWORK_FEE_PCT as u8),
			minimum_add_funds_amount: BTreeMap::default(),
			min_lending_pool_share: Percent::from_percent(30),
		});

		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FUNDS,
		));

		frame_system::Pallet::<Test>::reset_events();

		assert_ok!(LendingPools::try_boosting(
			DEPOSIT_ID,
			BOOST_ASSET,
			DEPOSIT_AMOUNT,
			BOOST_FEE_BPS
		));

		const TOTAL_BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000;
		const NETWORK_FEE: AssetAmount = TOTAL_BOOST_FEE * NETWORK_FEE_PCT / 100;
		const POOL_FEE: AssetAmount = TOTAL_BOOST_FEE - NETWORK_FEE;
		const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_BOOST_FEE;

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
				loan_id: LOAN_ID,
				loan_type: LoanType::Boost(DEPOSIT_ID),
				asset: BOOST_ASSET,
				principal_amount: REQUIRED_AMOUNT,
			}),
			RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
				loan_id: LOAN_ID,
				pool_fee: POOL_FEE,
				network_fee: NETWORK_FEE,
				broker_fee: 0,
			}),
		);

		assert_eq!(
			BoostLoans::<Test>::get(LOAN_ID),
			Some(GeneralLoan {
				id: LOAN_ID,
				asset: BOOST_ASSET,
				created_at_block: 1,
				owed_principal: DEPOSIT_AMOUNT,
				pending_interest: Default::default(),
			})
		);

		assert!(
			BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID).is_some(),
			"deposit must be boosted"
		);

		frame_system::Pallet::<Test>::reset_events();

		LendingPools::process_deposit_as_lost(DEPOSIT_ID, BOOST_ASSET);

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: DEPOSIT_AMOUNT,
				via_liquidation: false,
			}),
		);

		assert_eq!(BoostLoans::<Test>::get(LOAN_ID), None);
		assert_eq!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID), None);

		// Lender loses the deposit amount (excluding the pool fee portion that it didn't provide)
		assert_eq!(
			get_supply_position(BOOST_ASSET, BOOSTER_1),
			Some(BOOST_FUNDS - DEPOSIT_AMOUNT + POOL_FEE)
		);
	});
}

#[test]
fn stop_boosting_with_legacy_pool() {
	new_test_ext().execute_with(|| {
		const BOOSTER_AMOUNT_1: AssetAmount = 500_000_000;
		const DEPOSIT_AMOUNT: AssetAmount = 250_000_000;
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);

		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: Percent::from_percent(0),
			minimum_add_funds_amount: BTreeMap::default(),
			min_lending_pool_share: Percent::from_percent(30),
		});

		MockBalance::credit_account(&BOOSTER_1, BOOST_ASSET, INIT_BOOSTER_ETH_BALANCE);

		assert_ok!(LendingPools::create_boost_pools(
			RuntimeOrigin::root(),
			vec![BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS }],
		));

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOSTER_AMOUNT_1,
			BOOST_FEE_BPS
		));

		assert_ok!(LendingPools::try_boosting(
			DEPOSIT_ID,
			BOOST_ASSET,
			DEPOSIT_AMOUNT,
			BOOST_FEE_BPS
		));

		assert_eq!(
			MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET),
			INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1
		);

		// Booster stops boosting and get the available portion of their funds immediately:
		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FEE_BPS
		));

		const BOOST_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000;
		const AVAILABLE_BOOST_AMOUNT: AssetAmount = BOOSTER_AMOUNT_1 - (DEPOSIT_AMOUNT - BOOST_FEE);
		assert_eq!(
			MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET),
			INIT_BOOSTER_ETH_BALANCE - BOOSTER_AMOUNT_1 + AVAILABLE_BOOST_AMOUNT
		);

		System::assert_last_event(RuntimeEvent::LendingPools(Event::StoppedBoosting {
			booster_id: BOOSTER_1,
			boost_pool: BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS },
			unlocked_amount: AVAILABLE_BOOST_AMOUNT,
			pending_boosts: BTreeSet::from_iter(vec![DEPOSIT_ID]),
		}));

		// Deposit is finalised, the booster gets their remaining funds from the pool:
		LendingPools::finalise_boost(DEPOSIT_ID, BOOST_ASSET);
		assert_eq!(
			MockBalance::get_balance(&BOOSTER_1, BOOST_ASSET),
			INIT_BOOSTER_ETH_BALANCE + BOOST_FEE
		);
	});
}

#[test]
fn add_legacy_boost_funds_below_minimum() {
	new_test_ext().execute_with(|| {
		const MINIMUM_ADD_FUNDS_AMOUNT: AssetAmount = 10_000;

		setup_legacy_boost_pools();

		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: Default::default(),
			minimum_add_funds_amount: BTreeMap::from([(BOOST_ASSET, MINIMUM_ADD_FUNDS_AMOUNT)]),
			min_lending_pool_share: Percent::from_percent(30),
		});

		assert_noop!(
			LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				MINIMUM_ADD_FUNDS_AMOUNT - 1,
				BOOST_FEE_BPS
			),
			crate::Error::<Test>::AmountBelowMinimum
		);

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			MINIMUM_ADD_FUNDS_AMOUNT,
			BOOST_FEE_BPS
		));
	});
}

#[test]
fn add_boost_funds_is_disabled_by_safe_mode() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;

		setup_legacy_boost_pools();

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
			add_boost_funds_enabled: false,
			..PalletSafeMode::code_green()
		});

		// Should not be able to add funds to the boost pool
		assert_noop!(
			LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				BOOST_FUNDS,
				BOOST_FEE_BPS
			),
			crate::Error::<Test>::AddBoostFundsDisabled
		);

		assert_eq!(get_available_amount_for_booster(BOOST_ASSET, BOOSTER_1), None);

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());

		// Should be able to add funds to the boost pool now that the safe mode is turned off
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FUNDS,
			BOOST_FEE_BPS
		));
		assert_eq!(get_available_amount_for_booster(BOOST_ASSET, BOOSTER_1), Some(BOOST_FUNDS));
	});
}

#[test]
fn stop_boosting_is_disabled_by_safe_mode() {
	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;

		setup_legacy_boost_pools();

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FUNDS,
			BOOST_FEE_BPS
		));

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
			stop_boosting_enabled: false,
			..PalletSafeMode::code_green()
		});

		// Should not be able to stop boosting
		assert_noop!(
			LendingPools::stop_boosting(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				BOOST_FEE_BPS
			),
			crate::Error::<Test>::StopBoostingDisabled
		);

		assert_eq!(get_available_amount_for_booster(BOOST_ASSET, BOOSTER_1), Some(BOOST_FUNDS));

		MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());

		// Should be able to stop boosting now that the safe mode is turned off
		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			BOOST_FEE_BPS
		));
		assert_eq!(get_available_amount_for_booster(BOOST_ASSET, BOOSTER_1), None);
	});
}

#[test]
fn test_create_boost_pools() {
	new_test_ext().execute_with(|| {
		// Make sure the pools do not exists already
		assert!(BoostPools::<Test>::get(BOOST_ASSET, BOOST_FEE_BPS).is_none());
		assert!(BoostPools::<Test>::get(Asset::Flip, BOOST_FEE_BPS).is_none());

		// Create all 3 pools in one go
		assert_ok!(Pallet::<Test>::create_boost_pools(
			RuntimeOrigin::root(),
			vec![
				BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS },
				BoostPoolId { asset: Asset::Flip, tier: BOOST_FEE_BPS },
			]
		));

		// // Check they now exist
		assert!(BoostPools::<Test>::get(BOOST_ASSET, BOOST_FEE_BPS).is_some());
		assert!(BoostPools::<Test>::get(Asset::Flip, BOOST_FEE_BPS).is_some());

		// Check that all 2 emitted the creation event
		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::BoostPoolCreated {
				boost_pool: BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS },
			}),
			RuntimeEvent::LendingPools(Event::BoostPoolCreated {
				boost_pool: BoostPoolId { asset: Asset::Flip, tier: BOOST_FEE_BPS },
			})
		);

		// Should not be able to create the same pool again
		assert_noop!(
			Pallet::<Test>::create_boost_pools(
				RuntimeOrigin::root(),
				vec![BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS }]
			),
			crate::Error::<Test>::PoolAlreadyExists
		);

		// Make sure it did not remove the existing boost pool
		assert!(BoostPools::<Test>::get(BOOST_ASSET, BOOST_FEE_BPS).is_some());

		// Should not be able to create a pool with a tier of 0
		assert_noop!(
			Pallet::<Test>::create_boost_pools(
				RuntimeOrigin::root(),
				vec![BoostPoolId { asset: BOOST_ASSET, tier: 0 }]
			),
			crate::Error::<Test>::InvalidBoostPoolTier
		);

		// Should not be able to create a pool with a tier other than BOOST_FEE_BPS
		assert_noop!(
			Pallet::<Test>::create_boost_pools(
				RuntimeOrigin::root(),
				vec![BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS + 1 }]
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
		setup_legacy_boost_pools();

		const ETH_AMOUNT_1: AssetAmount = 50_000;
		const FLIP_AMOUNT: AssetAmount = 5_000;
		const BOOSTED_AMOUNT: AssetAmount = 20_000;

		// Add funds to two different pools:
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			ETH_AMOUNT_1,
			BOOST_FEE_BPS
		));

		// Add funds in a different asset to check that we
		// can distinguish between different assets:
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Flip,
			FLIP_AMOUNT,
			BOOST_FEE_BPS
		));

		// Add a different booster to make sure that their funds
		// don't affect the result for BOOSTER_1:
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			BOOST_ASSET,
			ETH_AMOUNT_1,
			BOOST_FEE_BPS
		));

		// A portion of the funds will is pending due to an unfinalised boost
		assert_ok!(LendingPools::try_boosting(
			PrewitnessedDepositId(0),
			BOOST_ASSET,
			BOOSTED_AMOUNT,
			BOOST_FEE_BPS
		));

		let boost_fee = BOOSTED_AMOUNT * BOOST_FEE_BPS as u128 / 10_000;

		// Booster 2 only gets half the fee (the other half goes to Booster 1):
		let booster_2_expected_balance = ETH_AMOUNT_1 + boost_fee / 2;

		// Check that we collect funds from all pools and include funds from unfinalised boosts,
		// ignoring other accounts and assets:
		assert_eq!(
			LendingPools::boost_pool_account_balance(&BOOSTER_1, BOOST_ASSET),
			booster_2_expected_balance
		);
	});
}

#[test]
fn deregistration_check_requires_no_lending_storage_keys() {
	new_test_ext().execute_with(|| {
		const BOOST_AMOUNT: AssetAmount = 1;

		assert_ok!(LendingPools::create_boost_pools(
			RuntimeOrigin::root(),
			vec![BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS }],
		));
		<Test as crate::Config>::Balance::credit_account(&LP, BOOST_ASSET, BOOST_AMOUNT);
		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(LP),
			BOOST_ASSET,
			BOOST_AMOUNT,
			BOOST_FEE_BPS
		));

		assert_noop!(
			PoolsDeregistrationCheck::<Test>::check(&LP),
			Error::<Test>::BoostedFundsRemaining
		);

		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(LP),
			BOOST_ASSET,
			BOOST_FEE_BPS
		));

		let mut lending_pool = LendingPool::new();
		lending_pool.add_funds(&LP, 1);
		GeneralLendingPools::<Test>::insert(BOOST_ASSET, lending_pool);

		assert_noop!(
			PoolsDeregistrationCheck::<Test>::check(&LP),
			Error::<Test>::LendingFundsRemaining
		);

		GeneralLendingPools::<Test>::remove(BOOST_ASSET);

		// Borrower loan-account storage keyed by account.
		LoanAccounts::<Test>::insert(LP, LoanAccount::<Test>::new(LP));

		assert_noop!(
			PoolsDeregistrationCheck::<Test>::check(&LP),
			Error::<Test>::LendingFundsRemaining
		);
	});
}

#[test]
fn boost_pool_details() {
	use crate::boost::{BoostPoolDetails, OwedAmount};

	new_test_ext().execute_with(|| {
		setup_legacy_boost_pools();
		const ETH_AMOUNT_1: AssetAmount = 50_000;
		const ETH_AMOUNT_2: AssetAmount = 25_000;
		const BOOSTED_AMOUNT: AssetAmount = 30_000;

		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(3);

		const NETWORK_FEE_DEDUCTION: Percent = Percent::from_percent(50);

		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: NETWORK_FEE_DEDUCTION,
			minimum_add_funds_amount: BTreeMap::default(),
			min_lending_pool_share: Percent::from_percent(30),
		});

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			BOOST_ASSET,
			ETH_AMOUNT_1,
			BOOST_FEE_BPS
		));

		assert_ok!(LendingPools::add_boost_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			BOOST_ASSET,
			ETH_AMOUNT_2,
			BOOST_FEE_BPS
		));

		assert_ok!(LendingPools::try_boosting(
			DEPOSIT_ID,
			BOOST_ASSET,
			BOOSTED_AMOUNT,
			BOOST_FEE_BPS
		));

		assert_ok!(LendingPools::stop_boosting(
			RuntimeOrigin::signed(BOOSTER_2),
			BOOST_ASSET,
			BOOST_FEE_BPS
		));

		assert_eq!(
			get_boost_pool_details::<Test>(BOOST_ASSET)
				.get(&BOOST_FEE_BPS)
				.cloned()
				.unwrap(),
			BoostPoolDetails {
				available_amounts: BTreeMap::from_iter([(BOOSTER_1, 30_010)]),
				pending_boosts: BTreeMap::from_iter([(
					DEPOSIT_ID,
					BTreeMap::from_iter([
						// Note the network fee deduction:
						(BOOSTER_1, OwedAmount { total: 20_000 - 5, fee: 5 }),
						(BOOSTER_2, OwedAmount { total: 10_000 - 2, fee: 3 })
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

/// Boosting with both lending and legacy boost pools
mod hybrid_boosting {

	use cf_traits::lending::BoostFinalisationOutcome;

	use super::*;

	// 20% of each pool's fee goes to the network; 80% stays in the pool.
	const NETWORK_FEE_PERCENT: u128 = 20;

	fn setup_both_pools() {
		MockPriceFeedApi::set_price_usd_fine(BOOST_ASSET, 1_000_000);
		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: Percent::from_percent(
				NETWORK_FEE_PERCENT as u8,
			),
			minimum_add_funds_amount: BTreeMap::default(),
			min_lending_pool_share: Percent::from_percent(30),
		});
		assert_ok!(LendingPools::update_whitelist(
			RuntimeOrigin::root(),
			WhitelistUpdate::SetAllowAll
		));

		assert_ok!(LendingPools::create_lending_pool(RuntimeOrigin::root(), BOOST_ASSET));

		assert_ok!(LendingPools::create_boost_pools(
			RuntimeOrigin::root(),
			vec![BoostPoolId { asset: BOOST_ASSET, tier: BOOST_FEE_BPS }],
		));
	}

	#[test]
	fn both_pools_participate() {
		new_test_ext().execute_with(|| {
			// Both pools have equal available liquidity, so the required amount is split 50/50.
			const DEPOSIT_AMOUNT: AssetAmount = 200_000_000;
			const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
			const LOAN_ID: LoanId = LoanId(0);

			const TOTAL_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000; // 100_000
			const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_FEE; // 199_900_000

			// Equal liquidity in both pools produces a clean 50/50 split.
			const LENDING_FUNDS: AssetAmount = REQUIRED_AMOUNT / 2; // 99_950_000
			const LEGACY_POOL_FUNDS: AssetAmount = REQUIRED_AMOUNT / 2; // 99_950_000

			// Total fee is split 50/50 between the two pools (each contributes half the principal).
			const LENDING_FEE_TOTAL: AssetAmount = TOTAL_FEE / 2; // 50_000
			const LEGACY_FEE_TOTAL: AssetAmount = TOTAL_FEE - LENDING_FEE_TOTAL; // 50_000

			const LENDING_NETWORK_FEE: AssetAmount = LENDING_FEE_TOTAL * NETWORK_FEE_PERCENT / 100; // 10_000
			const LENDING_POOL_FEE: AssetAmount = LENDING_FEE_TOTAL - LENDING_NETWORK_FEE; // 40_000
			const LEGACY_NETWORK_FEE: AssetAmount = LEGACY_FEE_TOTAL * NETWORK_FEE_PERCENT / 100; // 10_000
			const LEGACY_POOL_FEE: AssetAmount = LEGACY_FEE_TOTAL - LEGACY_NETWORK_FEE; // 40_000

			setup_both_pools();

			// 1. Fund the lending pool (won't be enough to cover the full required amount).
			MockBalance::credit_account(&BOOSTER_1, BOOST_ASSET, LENDING_FUNDS);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				LENDING_FUNDS,
			));

			// 2. Fund the legacy boost pool.
			MockBalance::credit_account(&BOOSTER_2, BOOST_ASSET, LEGACY_POOL_FUNDS);
			assert_ok!(LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_2),
				BOOST_ASSET,
				LEGACY_POOL_FUNDS,
				BOOST_FEE_BPS,
			));

			frame_system::Pallet::<Test>::reset_events();

			// 3. Boost deposit.
			assert_ok!(LendingPools::try_boosting(
				DEPOSIT_ID,
				BOOST_ASSET,
				DEPOSIT_AMOUNT,
				BOOST_FEE_BPS,
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					loan_type: LoanType::Boost(DEPOSIT_ID),
					asset: BOOST_ASSET,
					principal_amount: LENDING_FUNDS,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee: LENDING_POOL_FEE,
					network_fee: LENDING_NETWORK_FEE,
					broker_fee: 0,
				}),
			);

			// Boost should have consumed all available funds:
			assert_eq!(GeneralLendingPools::<Test>::get(BOOST_ASSET).unwrap().available_amount, 0);

			// Because lending pool has 0 available funds, its network fee portion is recorded
			// but not yet credited to the network:
			assert_eq!(
				GeneralLendingPools::<Test>::get(BOOST_ASSET).unwrap().owed_to_network,
				LENDING_NETWORK_FEE
			);

			assert_eq!(
				BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID),
				Some(BoostedDeposit {
					deposit_amount: DEPOSIT_AMOUNT,
					lending_loan_id: Some(LOAN_ID),
					boost_pool_contribution: Some(BoostPoolContribution {
						core_pool_id: CorePoolId(0),
						loan_id: CoreLoanId(0),
						boosted_amount: (REQUIRED_AMOUNT - LENDING_FUNDS) + LEGACY_FEE_TOTAL,
						network_fee: LEGACY_NETWORK_FEE,
					}),
				})
			);

			frame_system::Pallet::<Test>::reset_events();

			// 4. Finalise deposit: legacy boost pool's portion of the network fee
			// will be returned to the ingress-egress pallet where it will be swapped into FLIP
			assert_eq!(
				LendingPools::finalise_boost(DEPOSIT_ID, BOOST_ASSET),
				BoostFinalisationOutcome { network_fee: LEGACY_NETWORK_FEE }
			);

			const LENDING_REPAYMENT: AssetAmount = LENDING_FUNDS + LENDING_FEE_TOTAL;
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount: LENDING_REPAYMENT,
					action_type: LoanRepaidActionType::BoostFinalisation,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: false,
				}),
			);

			assert_eq!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID), None);

			// Lending pool earns the lending pool's portion of the fee (excludes the network fee).
			assert_eq!(
				get_supply_position(BOOST_ASSET, BOOSTER_1),
				Some(LENDING_FUNDS + LENDING_POOL_FEE)
			);
			// Boost pool earns the boost pool's portion of the fee (excludes the network fee).
			assert_eq!(
				get_available_amount_for_booster(BOOST_ASSET, BOOSTER_2),
				Some(LEGACY_POOL_FUNDS + LEGACY_POOL_FEE)
			);
		});
	}

	#[test]
	fn only_lending_pool_participates() {
		new_test_ext().execute_with(|| {
			// Boost pool exists but has no funds; lending pool covers everything.
			const DEPOSIT_AMOUNT: AssetAmount = 200_000_000;
			const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
			const LOAN_ID: LoanId = LoanId(0);

			const TOTAL_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000; // 100_000
			const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_FEE; // 199_900_000

			// Lending pool is funded with the full deposit amount — more than enough.
			const LENDING_FUNDS: AssetAmount = DEPOSIT_AMOUNT; // 200_000_000

			// Lending pool covers everything; boost pool contributes nothing.
			const LENDING_FEE_TOTAL: AssetAmount = TOTAL_FEE; // 100_000

			const LENDING_NETWORK_FEE: AssetAmount = LENDING_FEE_TOTAL * NETWORK_FEE_PERCENT / 100; // 20_000
			const LENDING_POOL_FEE: AssetAmount = LENDING_FEE_TOTAL - LENDING_NETWORK_FEE; // 80_000

			setup_both_pools();

			// Fund only the lending pool; leave the boost pool empty.
			MockBalance::credit_account(&BOOSTER_1, BOOST_ASSET, LENDING_FUNDS);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				LENDING_FUNDS,
			));

			frame_system::Pallet::<Test>::reset_events();

			// Boost deposit — lending pool covers the full required_amount.
			assert_ok!(LendingPools::try_boosting(
				DEPOSIT_ID,
				BOOST_ASSET,
				DEPOSIT_AMOUNT,
				BOOST_FEE_BPS,
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					loan_type: LoanType::Boost(DEPOSIT_ID),
					asset: BOOST_ASSET,
					principal_amount: REQUIRED_AMOUNT,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee: LENDING_POOL_FEE,
					network_fee: LENDING_NETWORK_FEE,
					broker_fee: 0,
				}),
			);

			assert_eq!(
				BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID),
				Some(BoostedDeposit {
					deposit_amount: DEPOSIT_AMOUNT,
					lending_loan_id: Some(LOAN_ID),
					boost_pool_contribution: None,
				})
			);

			// Lending pool had slack (LENDING_FUNDS > REQUIRED_AMOUNT), so the network fee
			// was collected immediately at boost time.
			assert_eq!(PendingNetworkFees::<Test>::get(BOOST_ASSET), LENDING_NETWORK_FEE);

			frame_system::Pallet::<Test>::reset_events();

			// 4. Finalise deposit — full deposit amount repays the lending loan.
			LendingPools::finalise_boost(DEPOSIT_ID, BOOST_ASSET);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount: DEPOSIT_AMOUNT,
					action_type: LoanRepaidActionType::BoostFinalisation,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: false,
				}),
			);

			assert_eq!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID), None);

			// Lending pool lender earns the net pool fee.
			assert_eq!(
				get_supply_position(BOOST_ASSET, BOOSTER_1),
				Some(LENDING_FUNDS + LENDING_POOL_FEE)
			);
		});
	}

	#[test]
	fn only_boost_pool_participates() {
		new_test_ext().execute_with(|| {
			// Lending pool exists but has no funds; boost pool covers everything.
			const DEPOSIT_AMOUNT: AssetAmount = 200_000_000;
			const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);

			const TOTAL_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000; // 100_000
			const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_FEE; // 199_900_000

			const BOOST_FUNDS: AssetAmount = DEPOSIT_AMOUNT;

			// Boost pool covers everything; lending pool contributes nothing.
			const BOOST_FEE_TOTAL: AssetAmount = TOTAL_FEE; // 100_000
			const BOOST_NETWORK_FEE: AssetAmount = BOOST_FEE_TOTAL * NETWORK_FEE_PERCENT / 100; // 20_000
			const BOOST_POOL_FEE: AssetAmount = BOOST_FEE_TOTAL - BOOST_NETWORK_FEE; // 80_000

			setup_both_pools();

			// Fund only the boost pool; leave the lending pool empty.
			MockBalance::credit_account(&BOOSTER_2, BOOST_ASSET, BOOST_FUNDS);
			assert_ok!(LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_2),
				BOOST_ASSET,
				BOOST_FUNDS,
				BOOST_FEE_BPS,
			));

			frame_system::Pallet::<Test>::reset_events();

			// Boost deposit — boost pool covers the full required_amount.
			assert_ok!(LendingPools::try_boosting(
				DEPOSIT_ID,
				BOOST_ASSET,
				DEPOSIT_AMOUNT,
				BOOST_FEE_BPS,
			));

			// No LoanCreated/OriginationFeeTaken events — lending pool was not used.
			assert_eq!(frame_system::Pallet::<Test>::events().len(), 0);

			assert_eq!(
				BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID),
				Some(BoostedDeposit {
					deposit_amount: DEPOSIT_AMOUNT,
					lending_loan_id: None,
					boost_pool_contribution: Some(BoostPoolContribution {
						core_pool_id: CorePoolId(0),
						loan_id: CoreLoanId(0),
						boosted_amount: REQUIRED_AMOUNT + BOOST_FEE_TOTAL,
						network_fee: BOOST_NETWORK_FEE,
					}),
				})
			);

			frame_system::Pallet::<Test>::reset_events();

			// Finalise deposit. Network fee is returned
			// to the ingress-egress pallet.
			assert_eq!(
				LendingPools::finalise_boost(DEPOSIT_ID, BOOST_ASSET),
				BoostFinalisationOutcome { network_fee: BOOST_NETWORK_FEE }
			);

			// No loan events — only boost pool was involved.
			assert_eq!(frame_system::Pallet::<Test>::events().len(), 0);

			assert_eq!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID), None);

			// Lending pool lender has nothing (it didn't participate).
			assert_eq!(get_supply_position(BOOST_ASSET, BOOSTER_1), None);
			// Boost pool booster earns the net pool fee.
			assert_eq!(
				get_available_amount_for_booster(BOOST_ASSET, BOOSTER_2),
				Some(BOOST_FUNDS + BOOST_POOL_FEE)
			);
		});
	}

	#[test]
	fn insufficient_liquidity_across_both_pools() {
		new_test_ext().execute_with(|| {
			const DEPOSIT_AMOUNT: AssetAmount = 200_000_000;
			const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);

			const TOTAL_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000; // 100_000
			const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_FEE; // 199_900_000

			// Together the two pools have less than required_amount.
			const LENDING_FUNDS: AssetAmount = REQUIRED_AMOUNT / 2 - 1;
			const BOOST_FUNDS: AssetAmount = REQUIRED_AMOUNT / 2 - 1;

			setup_both_pools();

			MockBalance::credit_account(&BOOSTER_1, BOOST_ASSET, LENDING_FUNDS);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				LENDING_FUNDS,
			));
			MockBalance::credit_account(&BOOSTER_2, BOOST_ASSET, BOOST_FUNDS);
			assert_ok!(LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_2),
				BOOST_ASSET,
				BOOST_FUNDS,
				BOOST_FEE_BPS,
			));

			assert_noop!(
				LendingPools::try_boosting(DEPOSIT_ID, BOOST_ASSET, DEPOSIT_AMOUNT, BOOST_FEE_BPS),
				Error::<Test>::InsufficientBoostLiquidity,
			);
		});
	}

	#[test]
	fn boosted_deposit_lost_with_both_pools() {
		new_test_ext().execute_with(|| {
			const DEPOSIT_AMOUNT: AssetAmount = 200_000_000;
			const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
			const LOAN_ID: LoanId = LoanId(0);

			const TOTAL_FEE: AssetAmount = DEPOSIT_AMOUNT * BOOST_FEE_BPS as u128 / 10_000; // 100_000
			const REQUIRED_AMOUNT: AssetAmount = DEPOSIT_AMOUNT - TOTAL_FEE; // 199_900_000

			// Equal liquidity in both pools => 50/50 split.
			const LENDING_FUNDS: AssetAmount = REQUIRED_AMOUNT / 2; // 99_950_000
			const LEGACY_POOL_FUNDS: AssetAmount = REQUIRED_AMOUNT / 2; // 99_950_000

			const LENDING_FEE_TOTAL: AssetAmount = TOTAL_FEE / 2; // 50_000

			const LENDING_NETWORK_FEE: AssetAmount = LENDING_FEE_TOTAL * NETWORK_FEE_PERCENT / 100; // 10_000

			// Total funds taken from the boost pool:
			const LEGACY_PRINCIPAL: AssetAmount = REQUIRED_AMOUNT - LENDING_FUNDS; // 99_950_000

			const LENDING_OWED_PRINCIPAL: AssetAmount = LENDING_FUNDS + LENDING_FEE_TOTAL; // 100_000_000

			setup_both_pools();

			MockBalance::credit_account(&BOOSTER_1, BOOST_ASSET, LENDING_FUNDS);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BOOSTER_1),
				BOOST_ASSET,
				LENDING_FUNDS,
			));
			MockBalance::credit_account(&BOOSTER_2, BOOST_ASSET, LEGACY_POOL_FUNDS);
			assert_ok!(LendingPools::add_boost_funds(
				RuntimeOrigin::signed(BOOSTER_2),
				BOOST_ASSET,
				LEGACY_POOL_FUNDS,
				BOOST_FEE_BPS,
			));

			assert_ok!(LendingPools::try_boosting(
				DEPOSIT_ID,
				BOOST_ASSET,
				DEPOSIT_AMOUNT,
				BOOST_FEE_BPS,
			));

			frame_system::Pallet::<Test>::reset_events();

			LendingPools::process_deposit_as_lost(DEPOSIT_ID, BOOST_ASSET);

			// Lending pool loan is settled with full owed_principal written off.
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: LENDING_OWED_PRINCIPAL,
					via_liquidation: false,
				}),
			);

			assert_eq!(BoostedDeposits::<Test>::get(BOOST_ASSET, DEPOSIT_ID), None);
			assert_eq!(BoostLoans::<Test>::get(LOAN_ID), None);

			// Lending pool lost all its funds:
			assert_eq!(get_supply_position(BOOST_ASSET, BOOSTER_1), Some(0));

			// The pool additionally owes some funds to the network (a side effect from
			// optimistically crediting network at the time of boosting):
			assert_eq!(
				GeneralLendingPools::<Test>::get(BOOST_ASSET).unwrap().owed_to_network,
				LENDING_NETWORK_FEE
			);

			// Boost pool booster lost the funds it provided for boosting (no network fee
			// had been taken):
			assert_eq!(
				get_available_amount_for_booster(BOOST_ASSET, BOOSTER_2),
				Some(LEGACY_POOL_FUNDS - LEGACY_PRINCIPAL),
			);
		});
	}
}

#[test]
fn get_all_loans_returns_boost_and_user_loans() {
	use cf_chains::ForeignChain;
	use cf_traits::mocks::balance_api::{MockBalance, MockLpRegistration};

	new_test_ext().execute_with(|| {
		const ETH_PRICE: u128 = 1;
		const BTC_PRICE: u128 = 20;

		assert_ok!(LendingPools::update_whitelist(
			RuntimeOrigin::root(),
			WhitelistUpdate::SetAllowAll
		));
		BoostConfig::<Test>::set(BoostConfiguration {
			network_fee_deduction_from_boost_percent: Percent::from_percent(0),
			minimum_add_funds_amount: BTreeMap::default(),
			min_lending_pool_share: Percent::from_percent(30),
		});
		MockPriceFeedApi::set_price_usd_fine(Asset::Btc, BTC_PRICE);
		MockPriceFeedApi::set_price_usd_fine(BOOST_ASSET, ETH_PRICE);

		// BTC pool funded for boost loans.
		const BOOST_DEPOSIT_AMOUNT: AssetAmount = 100_000_000;
		const BOOST_LENDER_FUNDS: AssetAmount = BOOST_DEPOSIT_AMOUNT * 2;
		assert_ok!(LendingPools::new_lending_pool(Asset::Btc));
		<Test as crate::Config>::Balance::credit_account(
			&BOOSTER_1,
			Asset::Btc,
			BOOST_LENDER_FUNDS,
		);
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BOOSTER_1),
			Asset::Btc,
			BOOST_LENDER_FUNDS,
		));

		// ETH pool funded for user loans.
		const PRINCIPAL: AssetAmount = 10_000_000_000;
		const ETH_LENDER_FUNDS: AssetAmount = PRINCIPAL * 2;
		assert_ok!(LendingPools::new_lending_pool(BOOST_ASSET));
		<Test as crate::Config>::Balance::credit_account(&BOOSTER_2, BOOST_ASSET, ETH_LENDER_FUNDS);
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BOOSTER_2),
			BOOST_ASSET,
			ETH_LENDER_FUNDS,
		));

		// Create a BTC boost loan.
		const DEPOSIT_ID: PrewitnessedDepositId = PrewitnessedDepositId(1);
		const BOOST_LOAN_ID: LoanId = LoanId(0);
		assert_ok!(LendingPools::try_boosting(
			DEPOSIT_ID,
			Asset::Btc,
			BOOST_DEPOSIT_AMOUNT,
			BOOST_FEE_BPS
		));

		// Create a user ETH loan collateralised with BTC (75% LTV).
		// LTV = (PRINCIPAL * ETH_PRICE) / (BTC_COLLATERAL * BTC_PRICE) = 0.75
		const BTC_COLLATERAL: AssetAmount = PRINCIPAL * 4 / (3 * BTC_PRICE);
		const USER_LOAN_ID: LoanId = LoanId(1);
		MockBalance::credit_account(&LP, Asset::Btc, BTC_COLLATERAL);
		MockLpRegistration::register_refund_address(LP, ForeignChain::Ethereum);
		assert_ok!(LendingPools::new_loan(
			LP,
			BOOST_ASSET,
			PRINCIPAL,
			None,
			BTreeMap::from([(Asset::Btc, BTC_COLLATERAL)]),
		));

		// Boost: owed_principal = required_amount + pool_fee + 0 network_fee =
		// BOOST_DEPOSIT_AMOUNT.
		const BOOST_PRINCIPAL: AssetAmount = BOOST_DEPOSIT_AMOUNT;
		// User loan: owed_principal = PRINCIPAL + origination_fee_total.
		// DEFAULT_ORIGINATION_FEE = Permill::from_parts(100) = 100 / 1_000_000.
		const ORIGINATION_FEE_TOTAL: AssetAmount = PRINCIPAL * 100 / 1_000_000;
		const USER_PRINCIPAL: AssetAmount = PRINCIPAL + ORIGINATION_FEE_TOTAL;

		let mut loans = get_all_loans::<Test>();
		loans.sort_by_key(|l| l.loan_id);

		assert_eq!(
			loans,
			vec![
				RpcLoan {
					loan_id: BOOST_LOAN_ID,
					loan_type: LoanType::Boost(DEPOSIT_ID),
					asset: Asset::Btc,
					created_at: 1,
					principal_amount: BOOST_PRINCIPAL,
				},
				RpcLoan {
					loan_id: USER_LOAN_ID,
					loan_type: LoanType::User(LP),
					asset: BOOST_ASSET,
					created_at: 1,
					principal_amount: USER_PRINCIPAL,
				},
			]
		);

		// After finalising the boost, only the user loan remains.
		LendingPools::finalise_boost(DEPOSIT_ID, Asset::Btc);
		let loans = get_all_loans::<Test>();
		assert_eq!(loans.len(), 1);
		assert_eq!(loans[0].loan_id, USER_LOAN_ID);
	});
}

#[test]
fn deregistration_check() {
	use cf_traits::{mocks::price_feed_api::MockPriceFeedApi, DeregistrationCheck};

	new_test_ext().execute_with(|| {
		const BOOST_FUNDS: AssetAmount = 500_000_000;
		const LENDER_FUNDS: AssetAmount = 1_000_000_000;

		setup_lending_pool_for_boost();

		// Disable whitelist for lending
		assert_ok!(LendingPools::update_whitelist(
			RuntimeOrigin::root(),
			WhitelistUpdate::SetAllowAll
		));

		// Set oracle prices for the assets
		MockPriceFeedApi::set_price_usd_fine(BOOST_ASSET, 1_000_000);
		MockPriceFeedApi::set_price_usd_fine(Asset::Flip, 1_000_000);

		// Credit LP with funds for both boost and lending
		<Test as crate::Config>::Balance::credit_account(&LP, BOOST_ASSET, BOOST_FUNDS);
		<Test as crate::Config>::Balance::credit_account(&LP, Asset::Flip, LENDER_FUNDS);

		assert_ok!(PoolsDeregistrationCheck::<Test>::check(&LP));

		// Test with lending funds: deregistration should fail when LP has active lending funds
		// First create a lending pool for the asset
		assert_ok!(LendingPools::new_lending_pool(Asset::Flip));

		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(LP),
			Asset::Flip,
			LENDER_FUNDS
		));

		assert!(matches!(
			PoolsDeregistrationCheck::<Test>::check(&LP),
			Err(Error::<Test>::LendingFundsRemaining)
		));

		// Remove lending funds - deregistration should succeed
		assert_ok!(LendingPools::remove_lender_funds(RuntimeOrigin::signed(LP), Asset::Flip, None));

		assert_ok!(PoolsDeregistrationCheck::<Test>::check(&LP));
	});
}
