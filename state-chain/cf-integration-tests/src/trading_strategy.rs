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

use cf_amm::math::price_at_tick;
use cf_primitives::{
	AccountId, AccountRole, Asset, AssetAmount, FLIPPERINOS_PER_FLIP, STABLE_ASSET,
};
use cf_test_utilities::assert_events_match;
use frame_support::assert_ok;
use state_chain_runtime::{AssetBalances, Runtime, RuntimeEvent, RuntimeOrigin};

type TradingStrategyPallet = state_chain_runtime::TradingStrategy;
use cf_traits::BalanceApi;
use pallet_cf_trading_strategy::TradingStrategy;

use crate::{
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
            new_pool(BASE_ASSET, 0, price_at_tick(0).unwrap());
            turn_off_thresholds();

            // Start trading strategy
            register_refund_addresses(&DORIS);
            credit_account(&DORIS, BASE_ASSET, AMOUNT);
            const STRATEGY: TradingStrategy =
                TradingStrategy::SellAndBuyAtTicks { sell_tick: 1, buy_tick: -1, base_asset: BASE_ASSET };
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
