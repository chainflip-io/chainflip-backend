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

use cf_primitives::{Asset, AssetAmount, Tick};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::{
		balance_api::{MockBalance, MockLpRegistration},
		pool_api::{MockLimitOrder, MockPoolApi},
	},
	BalanceApi, Side,
};
use frame_support::{assert_err, assert_ok};

use crate::{mock::*, *};

const BASE_ASSET: Asset = Asset::Usdt;
const QUOTE_ASSET: Asset = cf_primitives::STABLE_ASSET;
const BASE_AMOUNT: AssetAmount = 100_000;
const QUOTE_AMOUNT: AssetAmount = 50_000;

const BUY_TICK: Tick = -1;
const SELL_TICK: Tick = 1;

type AccountId = u64;

const STRATEGY: TradingStrategy = TradingStrategy::SellAndBuyAtTicks {
	sell_tick: SELL_TICK,
	buy_tick: BUY_TICK,
	base_asset: BASE_ASSET,
};

macro_rules! assert_balances {
	($account_id:expr, $base_amount:expr, $quote_amount:expr) => {
		assert_eq!(
			(
				MockBalance::get_balance(&$account_id, BASE_ASSET),
				MockBalance::get_balance(&$account_id, QUOTE_ASSET)
			),
			($base_amount, $quote_amount)
		);
	};
}

fn deploy_strategy() -> AccountId {
	let initial_amounts: BTreeMap<_, _> =
		[(BASE_ASSET, BASE_AMOUNT), (QUOTE_ASSET, QUOTE_AMOUNT)].into();

	for (asset, amount) in initial_amounts.clone() {
		MockLpRegistration::register_refund_address(LP, asset.into());
		MockBalance::credit_account(&LP, asset, amount);
	}

	assert_ok!(TradingStrategyPallet::deploy_strategy(
		RuntimeOrigin::signed(LP),
		STRATEGY.clone(),
		initial_amounts.clone(),
	));

	// An entry for the trading agent is created:
	let (lp_id, strategy_id, strategy) = Strategies::<Test>::iter().next().unwrap();
	assert_eq!(strategy, STRATEGY);
	assert_eq!(lp_id, LP);

	assert!(frame_system::Account::<Test>::contains_key(strategy_id), "Account not created");

	assert_event_sequence!(
		Test,
		RuntimeEvent::System(frame_system::Event::NewAccount { .. }),
		RuntimeEvent::TradingStrategyPallet(Event::<Test>::StrategyDeployed {
			account_id: LP,
			strategy_id: id,
			strategy: STRATEGY,
		}) if id == strategy_id,
		RuntimeEvent::TradingStrategyPallet(Event::<Test>::FundsAddedToStrategy {
			strategy_id: id,
			amounts: ref amounts_in_event

		}) if id == strategy_id && amounts_in_event == &initial_amounts,
	);

	// The funds are moved from the LP to the strategy:
	assert_balances!(strategy_id, BASE_AMOUNT, QUOTE_AMOUNT);
	assert_balances!(LP, 0, 0);

	strategy_id
}

fn check_asset_validation(f: impl Fn(BTreeMap<Asset, u128>) -> DispatchResult) {
	MockBalance::credit_account(&LP, BASE_ASSET, BASE_AMOUNT * 10);
	MockBalance::credit_account(&LP, QUOTE_ASSET, QUOTE_AMOUNT * 10);

	// These attempts should fail due to invalid assets provided:
	assert_err!(f(BTreeMap::from_iter([])), Error::<Test>::InvalidAssetsForStrategy);
	assert_err!(
		f(BTreeMap::from_iter([(Asset::Flip, 1000)])),
		Error::<Test>::InvalidAssetsForStrategy
	);
	assert_err!(
		f(BTreeMap::from_iter([(QUOTE_ASSET, QUOTE_AMOUNT), (Asset::Flip, 1000)])),
		Error::<Test>::InvalidAssetsForStrategy
	);
	assert_err!(
		f(BTreeMap::from_iter([
			(QUOTE_ASSET, QUOTE_AMOUNT),
			(BASE_ASSET, BASE_AMOUNT),
			(Asset::Flip, 1000)
		])),
		Error::<Test>::InvalidAssetsForStrategy
	);

	// Should be OK to provide one of &the assets (or both):
	assert_ok!(f(BTreeMap::from_iter([(QUOTE_ASSET, QUOTE_AMOUNT)])));
	assert_ok!(f(BTreeMap::from_iter([(BASE_ASSET, BASE_AMOUNT)])));
	assert_ok!(f(BTreeMap::from_iter([(QUOTE_ASSET, QUOTE_AMOUNT), (BASE_ASSET, BASE_AMOUNT)])));
}

#[test]
fn asset_validation_on_deploy_strategy() {
	new_test_ext().then_execute_at_next_block(|_| {
		MockLpRegistration::register_refund_address(LP, BASE_ASSET.into());
		MockLpRegistration::register_refund_address(LP, QUOTE_ASSET.into());

		check_asset_validation(|funding| {
			TradingStrategyPallet::deploy_strategy(
				RuntimeOrigin::signed(LP),
				STRATEGY.clone(),
				funding,
			)
		});
	});
}

#[test]
fn asset_validation_on_adding_funds_to_strategy() {
	new_test_ext().then_execute_at_next_block(|_| {
		let strategy_id = deploy_strategy();

		check_asset_validation(|funding| {
			TradingStrategyPallet::add_funds_to_strategy(
				RuntimeOrigin::signed(LP),
				strategy_id,
				funding,
			)
		});
	});
}

#[test]
fn refund_addresses_are_required() {
	new_test_ext().then_execute_at_next_block(|_| {
		// Using base asset that's on a different chain to make sure that
		// two different refund addresses are required:
		let base_asset = Asset::ArbUsdc;

		MockBalance::credit_account(&LP, base_asset, BASE_AMOUNT);
		MockBalance::credit_account(&LP, QUOTE_ASSET, QUOTE_AMOUNT);

		let deploy = || {
			TradingStrategyPallet::deploy_strategy(
				RuntimeOrigin::signed(LP),
				TradingStrategy::SellAndBuyAtTicks {
					sell_tick: SELL_TICK,
					buy_tick: BUY_TICK,
					base_asset,
				},
				[(base_asset, BASE_AMOUNT), (QUOTE_ASSET, QUOTE_AMOUNT)].into(),
			)
		};

		// Should fail since no assets are registered:
		assert_err!(deploy(), DispatchError::Other("no refund address"));

		// Registering a single asset should not be sufficient:
		MockLpRegistration::register_refund_address(LP, base_asset.into());

		assert_err!(deploy(), DispatchError::Other("no refund address"));

		// Should be able to deploy a strategy after registering the second asset:
		MockLpRegistration::register_refund_address(LP, QUOTE_ASSET.into());

		assert_ok!(deploy());
	});
}

#[test]
fn automated_strategy_basic_usage() {
	const ADDITIONAL_BASE_AMOUNT: AssetAmount = 5_000;

	new_test_ext()
		.then_execute_at_next_block(|_| deploy_strategy())
		.then_execute_at_next_block(|strategy_id| {
			// The strategy should have created two limit orders:
			assert_eq!(
				MockPoolApi::get_limit_orders(),
				vec![
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Buy,
						order_id: STRATEGY_ORDER_ID,
						tick: BUY_TICK,
						amount: QUOTE_AMOUNT
					},
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Sell,
						order_id: STRATEGY_ORDER_ID,
						tick: SELL_TICK,
						amount: BASE_AMOUNT
					}
				]
			);

			let amounts_to_add: BTreeMap<_, _> = [(BASE_ASSET, ADDITIONAL_BASE_AMOUNT)].into();

			// Add additional funds by calling the add funds extrinsic.
			MockBalance::credit_account(&LP, BASE_ASSET, ADDITIONAL_BASE_AMOUNT);
			assert_ok!(TradingStrategyPallet::add_funds_to_strategy(
				RuntimeOrigin::signed(LP),
				strategy_id,
				amounts_to_add.clone()
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::TradingStrategyPallet(Event::<Test>::FundsAddedToStrategy {
					strategy_id: id,
					amounts: ref amounts_in_event

				}) if id == strategy_id && amounts_in_event == &amounts_to_add,
			);

			// Update the threshold to check that limit orders won't be updated
			// if the threshold is not reached:
			LimitOrderUpdateThresholds::<Test>::mutate(|thresholds| {
				thresholds.insert(BASE_ASSET, ADDITIONAL_BASE_AMOUNT * 2);
			});

			assert_balances!(LP, 0, 0);
			assert_balances!(strategy_id, ADDITIONAL_BASE_AMOUNT, 0);

			strategy_id
		})
		.then_execute_at_next_block(|strategy_id| {
			// The funds have not been added to the limit order yet
			assert_balances!(strategy_id, ADDITIONAL_BASE_AMOUNT, 0);

			// This time we credit the strategy directly (which is what would happen
			// if our limit order is executed in the pools pallet). Now the strategy
			// should have enough free balance in BASE ASSET to update the limit order:
			MockBalance::credit_account(&strategy_id, BASE_ASSET, ADDITIONAL_BASE_AMOUNT);

			strategy_id
		})
		.then_execute_at_next_block(|strategy_id| {
			// The should now have been used to update the limit order:
			assert_balances!(strategy_id, 0, 0);

			assert_eq!(
				MockPoolApi::get_limit_orders(),
				vec![
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Buy,
						order_id: STRATEGY_ORDER_ID,
						tick: BUY_TICK,
						amount: QUOTE_AMOUNT
					},
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Sell,
						order_id: STRATEGY_ORDER_ID,
						tick: SELL_TICK,
						amount: BASE_AMOUNT + ADDITIONAL_BASE_AMOUNT * 2
					}
				]
			);
		});
}

#[test]
fn closing_strategy() {
	const ADDITIONAL_BASE_AMOUNT: AssetAmount = 5_000;
	new_test_ext()
		.then_execute_at_next_block(|_| deploy_strategy())
		.then_execute_at_next_block(|strategy_id| {
			// Two orders must have been created:
			assert_eq!(MockPoolApi::get_limit_orders().len(), 2);

			// Credit the strategy account so has a non-zero free balance:
			MockBalance::credit_account(&LP, BASE_ASSET, ADDITIONAL_BASE_AMOUNT);
			assert_balances!(LP, ADDITIONAL_BASE_AMOUNT, 0);

			// Closing the strategy
			assert_ok!(TradingStrategyPallet::close_strategy(
				RuntimeOrigin::signed(LP),
				strategy_id
			));

			assert!(
				!frame_system::Account::<Test>::contains_key(strategy_id),
				"Account not deleted"
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::System(frame_system::Event::KilledAccount { .. }),
				RuntimeEvent::TradingStrategyPallet(Event::<Test>::StrategyClosed {
					strategy_id: id,
				}) if id == strategy_id,
			);

			// Limit orders should be closed:
			assert!(MockPoolApi::get_limit_orders().is_empty());
			assert_eq!(Strategies::<Test>::iter().count(), 0);
			assert_balances!(strategy_id, 0, 0);
		});
}

#[test]
fn strategy_deployment_threshold() {
	const MIN_BASE_AMOUNT: AssetAmount = 10_000;
	const MIN_QUOTE_AMOUNT: AssetAmount = 1_000;

	new_test_ext().then_execute_at_next_block(|_| {
		MockBalance::credit_account(&LP, BASE_ASSET, MIN_BASE_AMOUNT * 10);
		MockBalance::credit_account(&LP, QUOTE_ASSET, MIN_QUOTE_AMOUNT * 10);

		MockLpRegistration::register_refund_address(LP, BASE_ASSET.into());
		MockLpRegistration::register_refund_address(LP, QUOTE_ASSET.into());

		MinimumDeploymentAmountForStrategy::<Test>::insert(BASE_ASSET, MIN_BASE_AMOUNT);
		MinimumDeploymentAmountForStrategy::<Test>::insert(QUOTE_ASSET, MIN_QUOTE_AMOUNT);

		for (base_amount, quote_amount) in [
			(MIN_BASE_AMOUNT - 1, 0),
			(0, MIN_QUOTE_AMOUNT - 1),
			(MIN_BASE_AMOUNT / 2 - 1, MIN_QUOTE_AMOUNT / 2),
			(MIN_BASE_AMOUNT / 10 - 1, (MIN_QUOTE_AMOUNT * 9) / 10),
		] {
			assert_err!(
				TradingStrategyPallet::deploy_strategy(
					RuntimeOrigin::signed(LP),
					STRATEGY.clone(),
					[(BASE_ASSET, base_amount), (QUOTE_ASSET, quote_amount)].into()
				),
				Error::<Test>::AmountBelowDeploymentThreshold
			);
		}

		for (base_amount, quote_amount) in [
			(MIN_BASE_AMOUNT, 0),
			(0, MIN_QUOTE_AMOUNT),
			(MIN_BASE_AMOUNT / 2, MIN_QUOTE_AMOUNT / 2),
			(MIN_BASE_AMOUNT / 10, (MIN_QUOTE_AMOUNT * 9) / 10),
		] {
			assert_ok!(TradingStrategyPallet::deploy_strategy(
				RuntimeOrigin::signed(LP),
				STRATEGY.clone(),
				[(BASE_ASSET, base_amount), (QUOTE_ASSET, quote_amount)].into()
			));
		}
	});
}

#[test]
fn deregistration_check() {
	new_test_ext()
		.then_execute_with(|_| deploy_strategy())
		.then_execute_with_keep_context(|_| {
			assert!(matches!(
				TradingStrategyDeregistrationCheck::<Test>::check(&LP),
				Err(Error::<Test>::LpHasActiveStrategies)
			));
		})
		.then_execute_with_keep_context(|strategy_id| {
			assert_ok!(TradingStrategyPallet::close_strategy(
				RuntimeOrigin::signed(LP),
				*strategy_id
			));

			assert_ok!(TradingStrategyDeregistrationCheck::<Test>::check(&LP));
		});
}
