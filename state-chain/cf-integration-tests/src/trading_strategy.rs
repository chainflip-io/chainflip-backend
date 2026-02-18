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

use std::collections::BTreeMap;

use cf_amm::math::Price;
use cf_primitives::{
	AccountId, AccountRole, Asset, AssetAmount, FLIPPERINOS_PER_FLIP, STABLE_ASSET,
};
use cf_test_utilities::{
	assert_events_match, assert_has_matching_event, assert_matching_event_count,
};
use frame_support::assert_ok;
use sp_std::collections::btree_set::BTreeSet;
use state_chain_runtime::{
	chainflip::ChainlinkOracle, AssetBalances, Runtime, RuntimeEvent, RuntimeOrigin, System,
};

type TradingStrategyPallet = state_chain_runtime::TradingStrategy;
use cf_traits::{BalanceApi, PoolApi, PriceFeedApi};
use pallet_cf_trading_strategy::TradingStrategy;

use crate::{
	advance_blocks,
	network::register_refund_addresses,
	swapping::{credit_account, do_eth_swap, new_pool},
};

const DORIS: AccountId = AccountId::new([0x11; 32]);
const ZION: AccountId = AccountId::new([0x22; 32]);
const BASE_ASSET: Asset = Asset::Usdt;
const QUOTE_ASSET: Asset = STABLE_ASSET;

fn turn_off_thresholds() {
	let zero_thresholds = BTreeMap::from_iter([(BASE_ASSET, 0), (QUOTE_ASSET, 0)]);
	pallet_cf_trading_strategy::MinimumDeploymentAmountForStrategy::<Runtime>::set(
		zero_thresholds.clone(),
	);
	pallet_cf_trading_strategy::MinimumAddedFundsToStrategy::<Runtime>::set(
		zero_thresholds.clone(),
	);
	pallet_cf_trading_strategy::LimitOrderUpdateThresholds::<Runtime>::set(zero_thresholds.clone());
}

#[test]
fn basic_usage() {
	const DECIMALS: u128 = 10u128.pow(6);
	const AMOUNT: AssetAmount = 10_000 * DECIMALS;

	super::genesis::with_test_defaults()
		.with_additional_accounts(&[(
			DORIS,
			AccountRole::LiquidityProvider,
			5 * FLIPPERINOS_PER_FLIP,
		),
		(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),])
		.build()
		.execute_with(|| {
			new_pool(BASE_ASSET, 0, Price::at_tick_zero());
			turn_off_thresholds();

			// Start trading strategy
			register_refund_addresses(&DORIS);
			credit_account(&DORIS, BASE_ASSET, AMOUNT);
			const STRATEGY: TradingStrategy =
				TradingStrategy::TickZeroCentered { spread_tick: 1, base_asset: BASE_ASSET };
			assert_ok!(TradingStrategyPallet::deploy_strategy(
				RuntimeOrigin::signed(DORIS),
				STRATEGY.clone(),
				BTreeMap::from_iter([(BASE_ASSET, AMOUNT)]),
			));

			// Get the strategy ID from the emitted event
			let strategy_id = assert_events_match!(
				Runtime,
				RuntimeEvent::TradingStrategy(
					pallet_cf_trading_strategy::Event::StrategyDeployed{account_id: DORIS, strategy_id, strategy: STRATEGY}) => strategy_id
			);

			// Add additional funds
			credit_account(&DORIS, BASE_ASSET, AMOUNT);
			assert_ok!(TradingStrategyPallet::add_funds_to_strategy(
				RuntimeOrigin::signed(DORIS),
				strategy_id.clone(),
				BTreeMap::from_iter([(BASE_ASSET, AMOUNT)]),
			));

			// Make sure all of our funds are in the strategy
			let balances = AssetBalances::free_balances(&DORIS);
			assert_eq!(balances[BASE_ASSET], 0);
			assert_eq!(balances[QUOTE_ASSET], 0);

			// Do a swap
			// NOTE: This will also run the on_idle logic needed to create the limit orders
			let (_egress_id, swap_input_amount, _swap_output_amount) = do_eth_swap(
				QUOTE_ASSET.try_into().unwrap(),
				BASE_ASSET.try_into().unwrap(),
				&ZION,
				AMOUNT,
			);

			// Stop the strategy
			assert_ok!(TradingStrategyPallet::close_strategy(RuntimeOrigin::signed(DORIS), strategy_id));

			// Check our balances
			let balances = AssetBalances::free_balances(&DORIS);
			assert!(
				balances[BASE_ASSET] > AMOUNT * 2 - swap_input_amount,
				"Should see increase due to tick"
			);
			const ROUNDING_ERROR: AssetAmount = 2;
			assert_eq!(balances[QUOTE_ASSET], swap_input_amount - ROUNDING_ERROR);

	});
}

#[test]
fn can_close_strategy_with_fully_executed_orders() {
	const DECIMALS: u128 = 10u128.pow(6);
	const AMOUNT: AssetAmount = 10_000 * DECIMALS;

	super::genesis::with_test_defaults()
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			new_pool(BASE_ASSET, 0, Price::at_tick_zero());
			turn_off_thresholds();

			// Start trading strategy
			register_refund_addresses(&DORIS);
			credit_account(&DORIS, BASE_ASSET, AMOUNT * 4);
			const STRATEGY: TradingStrategy =
				TradingStrategy::TickZeroCentered { spread_tick: 1, base_asset: BASE_ASSET };
			assert_ok!(TradingStrategyPallet::deploy_strategy(
				RuntimeOrigin::signed(DORIS),
				STRATEGY.clone(),
				BTreeMap::from_iter([(BASE_ASSET, AMOUNT)]),
			));

			// Get the strategy ID from the emitted event
			let strategy_id = assert_events_match!(
				Runtime,
				RuntimeEvent::TradingStrategy(
					pallet_cf_trading_strategy::Event::StrategyDeployed{account_id: DORIS, strategy_id, strategy: STRATEGY}) => strategy_id
			);

			// Setting a LO alongside the strategy to make it easier to fully
			// consume the strategy's order
			assert_ok!(state_chain_runtime::LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(DORIS),
				BASE_ASSET,
				Asset::Usdc,
				cf_amm::common::Side::Sell,
				0,
				Some(10),
				AMOUNT * 3,
				None,
				None
			));

			// This swap will fully consume strategy's order so it will be removed
			// upon sweeping
			let (_egress_id, _swap_input_amount, _swap_output_amount) = do_eth_swap(
				QUOTE_ASSET.try_into().unwrap(),
				BASE_ASSET.try_into().unwrap(),
				&ZION,
				AMOUNT * 2,
			);


			// In the past this would fail with "OrderDoesNotExist" since implicit
			// sweeping (executed before every order update) was pre-removing it
			// and we were effectively trying to remove the same order twice.
			assert_ok!(TradingStrategyPallet::close_strategy(
				RuntimeOrigin::signed(DORIS),
				strategy_id
			));
		});
}

#[test]
fn inventory_based_strategy_basic_usage() {
	const DECIMALS: u128 = 10u128.pow(6);
	const AMOUNT: AssetAmount = 10_000 * DECIMALS;

	super::genesis::with_test_defaults()
        .with_additional_accounts(&[(
            DORIS,
            AccountRole::LiquidityProvider,
            5 * FLIPPERINOS_PER_FLIP,
        ),
        (ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),])
        .build()
        .execute_with(|| {
            new_pool(BASE_ASSET, 0, Price::at_tick_zero());
            turn_off_thresholds();

            // Start trading strategy
            register_refund_addresses(&DORIS);
            credit_account(&DORIS, BASE_ASSET, AMOUNT);
            const STRATEGY: TradingStrategy =
                TradingStrategy::InventoryBased { base_asset: BASE_ASSET, min_buy_tick: -10, max_buy_tick:  2, min_sell_tick: 2, max_sell_tick: 10};
            assert_ok!(TradingStrategyPallet::deploy_strategy(
                RuntimeOrigin::signed(DORIS),
                STRATEGY.clone(),
                BTreeMap::from_iter([(BASE_ASSET, AMOUNT)]),
            ));

            // Get the strategy ID from the emitted event
            let strategy_id = assert_events_match!(
                Runtime,
                RuntimeEvent::TradingStrategy(
                    pallet_cf_trading_strategy::Event::StrategyDeployed{account_id: DORIS, strategy_id, strategy: STRATEGY}) => strategy_id
            );

            // Add additional funds
            credit_account(&DORIS, BASE_ASSET, AMOUNT);
            assert_ok!(TradingStrategyPallet::add_funds_to_strategy(
                RuntimeOrigin::signed(DORIS),
                strategy_id.clone(),
                BTreeMap::from_iter([(BASE_ASSET, AMOUNT)]),
            ));

            // Make sure all of our funds are in the strategy
            let balances = AssetBalances::free_balances(&DORIS);
            assert_eq!(balances[BASE_ASSET], 0);
            assert_eq!(balances[QUOTE_ASSET], 0);

            // Do a swap
            // NOTE: This will also run the on_idle logic needed to create the limit orders
            let (_egress_id, swap_input_amount, _swap_output_amount) = do_eth_swap(
                QUOTE_ASSET.try_into().unwrap(),
                BASE_ASSET.try_into().unwrap(),
                &ZION,
                AMOUNT,
            );

            // Stop the strategy
            assert_ok!(TradingStrategyPallet::close_strategy(RuntimeOrigin::signed(DORIS), strategy_id));
            // Check our balances
            let balances = AssetBalances::free_balances(&DORIS);
            assert!(
                balances[BASE_ASSET] > AMOUNT * 2 - swap_input_amount,
                "Should see increase due to tick",
            );
            const ROUNDING_ERROR: AssetAmount = 2;
            assert_eq!(balances[QUOTE_ASSET], swap_input_amount - ROUNDING_ERROR);
        });
}
#[test]
fn oracle_strategy_basic_usage() {
	const DECIMALS: u128 = 10u128.pow(6);
	const AMOUNT: AssetAmount = 10_000 * DECIMALS;
	const STARTING_PRICE_CENTS: u32 = 99;

	super::genesis::with_test_defaults()
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			new_pool(BASE_ASSET, 0, Price::at_tick_zero());
			turn_off_thresholds();

			// Set the oracle prices
			ChainlinkOracle::set_price(
				BASE_ASSET,
				Price::from_usd_cents(BASE_ASSET, STARTING_PRICE_CENTS),
			);
			ChainlinkOracle::set_price(STABLE_ASSET, Price::from_usd_cents(STABLE_ASSET, 101));

			// Start trading strategy
			register_refund_addresses(&DORIS);
			credit_account(&DORIS, BASE_ASSET, AMOUNT);
			const STRATEGY: TradingStrategy = TradingStrategy::OracleTracking {
				base_asset: BASE_ASSET,
				quote_asset: STABLE_ASSET,
				min_buy_offset_tick: -10,
				max_buy_offset_tick: 2,
				min_sell_offset_tick: 2,
				max_sell_offset_tick: 10,
			};
			assert_ok!(TradingStrategyPallet::deploy_strategy(
				RuntimeOrigin::signed(DORIS),
				STRATEGY.clone(),
				BTreeMap::from_iter([(BASE_ASSET, AMOUNT)]),
			));

			// Get the strategy ID from the emitted event
			let strategy_id = assert_events_match!(
				Runtime,
				RuntimeEvent::TradingStrategy(pallet_cf_trading_strategy::Event::StrategyDeployed {
					account_id: DORIS,
					strategy_id,
					strategy: STRATEGY
				}) => strategy_id
			);

			// Make sure the limit orders opened
			System::reset_events();
			advance_blocks(1);
			assert_matching_event_count!(
				Runtime,
				RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated { .. }) => 2
			);
			assert_eq!(
				state_chain_runtime::LiquidityPools::limit_orders(
					BASE_ASSET,
					STABLE_ASSET,
					&BTreeSet::from([strategy_id.clone()]),
				)
				.unwrap()
				.len(),
				2
			);

			// Make sure the orders did not update without a reason
			System::reset_events();
			advance_blocks(1);
			assert_matching_event_count!(
				Runtime,
				RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated { .. }) => 0
			);

			// Change a price to trigger an update
			System::reset_events();
			ChainlinkOracle::set_price(
				BASE_ASSET,
				Price::from_usd_cents(BASE_ASSET, STARTING_PRICE_CENTS - 1),
			);
			advance_blocks(1);

			// Orders at the old ticks are removed and new ones are added at the new ticks
			const ORDER_AMOUNT: AssetAmount = AMOUNT / 2;
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated {
					sell_amount_change: Some(cf_traits::IncreaseOrDecrease::Decrease(ORDER_AMOUNT)),
					tick: -199,
					..
				})
			);
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated {
					sell_amount_change: Some(cf_traits::IncreaseOrDecrease::Decrease(ORDER_AMOUNT)),
					tick: -195,
					..
				})
			);
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated {
					sell_amount_change: Some(cf_traits::IncreaseOrDecrease::Increase(ORDER_AMOUNT)),
					tick: -296,
					..
				})
			);
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated {
					sell_amount_change: Some(cf_traits::IncreaseOrDecrease::Increase(ORDER_AMOUNT)),
					tick: -300,
					..
				})
			);

			// Do a swap
			let _ = do_eth_swap(
				QUOTE_ASSET.try_into().unwrap(),
				BASE_ASSET.try_into().unwrap(),
				&ZION,
				AMOUNT / 2,
			);

			// Stop the strategy
			assert_ok!(TradingStrategyPallet::close_strategy(
				RuntimeOrigin::signed(DORIS),
				strategy_id
			));

			// Check our balances
			let balances = AssetBalances::free_balances(&DORIS);
			let starting_value =
				ChainlinkOracle::get_price(BASE_ASSET).unwrap().price.output_amount_ceil(AMOUNT);
			let base_value = ChainlinkOracle::get_price(BASE_ASSET)
				.unwrap()
				.price
				.output_amount_ceil(balances[BASE_ASSET]);
			let quote_value = ChainlinkOracle::get_price(QUOTE_ASSET)
				.unwrap()
				.price
				.output_amount_ceil(balances[QUOTE_ASSET]);
			assert!(base_value + quote_value > starting_value, "Should see increase due to tick",);
		});
}
