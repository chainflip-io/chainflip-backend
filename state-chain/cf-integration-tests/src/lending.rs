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

use frame_support::traits::Time;
use pallet_cf_lending_pools::{GeneralLendingPools, WhitelistStatus};
use pallet_cf_swapping::SwapRequestCompletionReason;
use state_chain_runtime::{
	chainflip::ChainlinkOracle, AssetBalances, LendingPools, Runtime, RuntimeEvent, RuntimeOrigin,
	Timestamp,
};
use std::collections::BTreeMap;

use crate::{
	network::register_refund_addresses,
	swapping::{credit_account, new_pool, set_limit_order},
	LIQUIDITY_PROVIDER,
};

use cf_amm::math::price_at_tick;
use cf_primitives::{
	AccountId, AccountRole, Asset, AssetAmount, DcaParameters, FLIPPERINOS_PER_FLIP,
	SWAP_DELAY_BLOCKS,
};
use cf_test_utilities::assert_has_matching_event;
use cf_traits::{lending::LendingApi, BalanceApi, PriceFeedApi};
use cf_utilities::assert_ok;

/// The main purpose of the following test is to check the interaction between the lending pallet
/// and the swapping pallets. To check that liquidation will create a swap and the funds will end up
/// back in the pool.
#[test]
fn basic_lending() {
	const LOAN_ASSET: Asset = Asset::Btc;
	const COLLATERAL_ASSET: Asset = Asset::Eth;

	const LP: AccountId = AccountId::new(LIQUIDITY_PROVIDER);
	const LENDER: AccountId = AccountId::new([0xf6; 32]);
	const BORROWER: AccountId = AccountId::new([0xf7; 32]);

	const POOL_FEE: u32 = 1000; // 10 bps

	const COLLATERAL_AMOUNT: AssetAmount = 150_000_000_000;
	const LOAN_AMOUNT: AssetAmount = 100_000_000_000;
	const LENDING_POOL_STARTING_AMOUNT: AssetAmount = LOAN_AMOUNT * 2;

	super::genesis::with_test_defaults()
		.with_additional_accounts(&[
			(LENDER, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(BORROWER, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			// Set the prices.
			let loan_price = price_at_tick(0).unwrap();
			let collateral_price = price_at_tick(0).unwrap();
			ChainlinkOracle::set_price(LOAN_ASSET, loan_price);
			ChainlinkOracle::set_price(COLLATERAL_ASSET, collateral_price);

			// Setup liquidity pools
			pallet_cf_lending_pools::Whitelist::<Runtime>::set(WhitelistStatus::AllowAll);
			new_pool(LOAN_ASSET, POOL_FEE, loan_price);
			new_pool(COLLATERAL_ASSET, POOL_FEE, collateral_price);
			register_refund_addresses(&LP);
			credit_account(&LP, LOAN_ASSET, COLLATERAL_AMOUNT * 10);
			credit_account(&LP, COLLATERAL_ASSET, COLLATERAL_AMOUNT * 10);
			credit_account(&LP, Asset::Usdc, COLLATERAL_AMOUNT * 10);
			set_limit_order(&LP, COLLATERAL_ASSET, Asset::Usdc, 0, Some(0), COLLATERAL_AMOUNT * 2);
			set_limit_order(&LP, Asset::Usdc, COLLATERAL_ASSET, 0, Some(0), COLLATERAL_AMOUNT * 2);
			set_limit_order(&LP, LOAN_ASSET, Asset::Usdc, 0, Some(0), COLLATERAL_AMOUNT * 2);
			set_limit_order(&LP, Asset::Usdc, LOAN_ASSET, 0, Some(0), COLLATERAL_AMOUNT * 2);

			// Setup a lending pool with some funds
			assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));
			register_refund_addresses(&LENDER);
			credit_account(&LENDER, LOAN_ASSET, LENDING_POOL_STARTING_AMOUNT);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER.clone()),
				LOAN_ASSET,
				LENDING_POOL_STARTING_AMOUNT,
			));

			// Setup the borrower account
			assert_eq!(AssetBalances::get_balance(&BORROWER, LOAN_ASSET), 0);
			assert_eq!(AssetBalances::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
			register_refund_addresses(&BORROWER);
			credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);

			// Add half the collateral first
			assert_ok!(<LendingPools as LendingApi>::add_collateral(
				&BORROWER,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_AMOUNT / 2)]),
			));

			// Now open a loan with the rest of the collateral
			assert_ok!(LendingPools::new_loan(
				BORROWER.clone(),
				LOAN_ASSET,
				LOAN_AMOUNT,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_AMOUNT / 2)]),
			));

			// Check that we got the loan amount
			assert_eq!(AssetBalances::get_balance(&BORROWER, LOAN_ASSET), LOAN_AMOUNT);
			assert_eq!(AssetBalances::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			// Now change the price so that the loan is liquidated
			ChainlinkOracle::set_price(COLLATERAL_ASSET, collateral_price / 2);
		})
		.then_execute_at_next_block(|_| {
			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		})
		.then_execute_with(|_| {
			// Check for the liquidation swap
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
					input_asset: COLLATERAL_ASSET,
					output_asset: LOAN_ASSET,
					input_amount: COLLATERAL_AMOUNT,
					dca_parameters: Some(DcaParameters { number_of_chunks: 2, chunk_interval: 1 }),
					..
				})
			);
		})
		.then_process_blocks_with(SWAP_DELAY_BLOCKS + 1, |_| {
			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		})
		.then_execute_with(|_| {
			// The first chunk should have brought the loan back into good standing and stopped the
			// liquidation
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequestCompleted {
					reason: SwapRequestCompletionReason::Aborted,
					..
				})
			);

			// Check that balances
			assert_eq!(AssetBalances::get_balance(&BORROWER, LOAN_ASSET), LOAN_AMOUNT);
			assert_eq!(AssetBalances::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
			let loan_account =
				pallet_cf_lending_pools::LoanAccounts::<Runtime>::get(&BORROWER).unwrap();
			// Half of the collateral was swapped to repay the loan
			let repaid_amount = COLLATERAL_AMOUNT / 2;
			assert_eq!(
				*loan_account.get_total_collateral().get(&COLLATERAL_ASSET).unwrap(),
				COLLATERAL_AMOUNT - repaid_amount
			);

			// The pool should have received partial liquidation amount minus fees
			let pool = GeneralLendingPools::<Runtime>::get(LOAN_ASSET).unwrap();
			assert!(
				pool.available_amount > LENDING_POOL_STARTING_AMOUNT - LOAN_AMOUNT &&
					pool.available_amount <
						LENDING_POOL_STARTING_AMOUNT - LOAN_AMOUNT + repaid_amount
			);
		});
}
