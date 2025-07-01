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

use cf_amm::math::price_at_tick;
use cf_primitives::{
	AccountId, AccountRole, Asset, AssetAmount, FLIPPERINOS_PER_FLIP, SWAP_DELAY_BLOCKS,
};
use cf_test_utilities::assert_has_matching_event;
use cf_utilities::assert_ok;
use frame_support::traits::Time;
use sp_core::bounded_vec;
use sp_runtime::{Perbill, Permill};
use state_chain_runtime::{
	AssetBalances, LendingPools, LiquidityPools, Runtime, RuntimeEvent, RuntimeOrigin, Timestamp,
};

use crate::{
	network::register_refund_addresses,
	swapping::{credit_account, new_pool},
	LIQUIDITY_PROVIDER,
};

use cf_traits::{
	lending::{ChpLendingApi, ChpLoanId},
	BalanceApi, Side,
};
use pallet_cf_lending_pools::ChpConfiguration;

/// The main purpose of the following test is to check the interaction between the lending pallet
/// and the swapping pallets: we create a loan which expires and requests a swap of collateral into
/// the borrowed asset.
#[test]
fn chp_lending() {
	const POOL_FEE: u32 = 1000; // 10 bps
	const ASSET: Asset = Asset::Btc;

	const LP: AccountId = AccountId::new(LIQUIDITY_PROVIDER);
	const LENDER: AccountId = AccountId::new([0xf6; 32]);
	const BORROWER: AccountId = AccountId::new([0xf7; 32]);

	const DURATION: u32 = 2;

	const LOAN_AMOUNT: AssetAmount = 1_000_000;
	const COLLATERAL_REQUIRED: AssetAmount = LOAN_AMOUNT + LOAN_AMOUNT / 5;
	const INITIAL_FEE: AssetAmount = 2_000;

	const LOAN_ID: ChpLoanId = ChpLoanId(0);

	type CorePools = pallet_cf_lending_pools::CorePools<Runtime>;
	type ChpLoans = pallet_cf_lending_pools::ChpLoans<Runtime>;

	super::genesis::with_test_defaults()
		.with_additional_accounts(&[(
			LENDER,
			AccountRole::LiquidityProvider,
			5 * FLIPPERINOS_PER_FLIP,
		)])
		.build()
		.execute_with(|| {
			assert_ok!(LendingPools::update_pallet_config(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				bounded_vec![pallet_cf_lending_pools::PalletConfigUpdate::SetChpConfig {
					config: ChpConfiguration {
						clearing_fee_base: Permill::from_rational(1u32, 1000), // 10 bps
						clearing_fee_utilisation_factor: Permill::from_rational(1u32, 1000), // 10 bps
						interest_base: Perbill::from_rational(1u32, 10_000),   // 1 bps
						interest_utilisation_factor: Perbill::from_rational(1u32, 10_000), // 1 bps
						overcollateralisation_target: Permill::from_percent(20),
						overcollateralisation_topup_threshold: Permill::from_percent(15),
						overcollateralisation_soft_threshold: Permill::from_percent(10),
						overcollateralisation_hard_threshold: Permill::from_percent(5),
						max_loan_duration: DURATION,
					}
				}],
			));

			register_refund_addresses(&LP);

			new_pool(ASSET, POOL_FEE, price_at_tick(0).unwrap());

			credit_account(&LP, Asset::Btc, 2_000_000);

			assert_ok!(LiquidityPools::set_limit_order(
				RuntimeOrigin::signed(LP.clone()),
				ASSET,
				Asset::Usdc,
				Side::Sell,
				0,
				Some(100),
				2_000_000,
				None, // Dispatch now
				None, // No expiration
			));

			assert_ok!(LendingPools::new_chp_pool(ASSET));

			credit_account(&LENDER, ASSET, 1_000_000);
			assert_ok!(LendingPools::add_chp_funds(
				RuntimeOrigin::signed(LENDER.clone()),
				ASSET,
				LOAN_AMOUNT,
			));

			assert_eq!(AssetBalances::get_balance(&BORROWER, ASSET), 0);
			assert_eq!(AssetBalances::get_balance(&BORROWER, Asset::Usdc), 0);
			credit_account(&BORROWER, Asset::Usdc, COLLATERAL_REQUIRED + INITIAL_FEE);

			assert_ok!(LendingPools::new_chp_loan(BORROWER.clone(), ASSET, LOAN_AMOUNT));

			assert!(ChpLoans::get(ASSET, LOAN_ID).is_some());

			assert_eq!(AssetBalances::get_balance(&BORROWER, ASSET), LOAN_AMOUNT);
			assert_eq!(AssetBalances::get_balance(&BORROWER, Asset::Usdc), 0);
		})
		.then_process_blocks_with(DURATION, |_| {
			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		})
		.then_execute_with(|_| {
			// A swap for collateral must have been scheduled
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
					input_asset: Asset::Usdc,
					output_asset: ASSET,
					input_amount: 1_200_000,
					..
				})
			);
		})
		.then_process_blocks_with(SWAP_DELAY_BLOCKS, |_| {
			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequestCompleted { .. })
			);

			// A swap for fees must have been scheduled
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
					input_asset: Asset::Usdc,
					output_asset: ASSET,
					input_amount: INITIAL_FEE,
					..
				})
			);
		})
		.then_process_blocks_with(SWAP_DELAY_BLOCKS, |_| {
			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		})
		.then_execute_with(|_| {
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequestCompleted { .. })
			);

			// Loan should be closed now:
			assert!(ChpLoans::get(ASSET, LOAN_ID).is_none());

			// The LP gets to keep their loaned asset plus whatever left after their collateral was
			// liquidated
			assert_eq!(AssetBalances::get_balance(&BORROWER, ASSET), LOAN_AMOUNT + 188059);
			assert_eq!(AssetBalances::get_balance(&BORROWER, Asset::Usdc), 0);

			assert_eq!(CorePools::iter().count(), 1);
			let core_pool = CorePools::iter().next().unwrap().2;

			// The pool should have the borrowed amount back plus some fees in the loan asset
			assert_eq!(core_pool.get_available_amount(), LOAN_AMOUNT + 1979);
		});
}
