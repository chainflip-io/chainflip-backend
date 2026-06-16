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
use cf_primitives::{AssetAmount, AssetAndAmount, Beneficiary, SwapRequestId};
use cf_traits::lending::LoanId;
use serde::{Deserialize, Serialize};
use sp_core::U256;

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLoan<AccountId, Amount> {
	pub loan_id: LoanId,
	pub loan_type: LoanType<AccountId>,
	pub asset: Asset,
	pub created_at: u32,
	pub principal_amount: Amount,
	pub broker: Option<Beneficiary<AccountId>>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLendingPool<Amount> {
	pub asset: Asset,
	/// Total amount collectively owed to lenders
	pub total_amount: Amount,
	/// Amount currently unused in loans. Not strictly the same as "available for new
	/// borrows" — the utilisation cap may restrict how much of this can actually be lent
	/// out.
	pub available_amount: Amount,
	pub utilisation_rate: Permill,
	/// Maximum utilisation allowed when opening new loans: borrows that would push utilisation
	/// above this cap are rejected so the pool retains enough liquidity to liquidate the
	/// configured fraction of outstanding loans at current oracle prices.
	pub utilisation_cap: Permill,
	pub current_interest_rate: Permill,
	#[serde(flatten)]
	pub config: LendingPoolConfiguration,
}

/// Total amount of funds (of some asset) owed by a lending pool to account `lp_id`.
#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct LendingSupplyPosition<AccountId, Amount> {
	pub lp_id: AccountId,
	pub total_amount: Amount,
}

/// All supply positions for a pool identified by `asset`.
#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct LendingPoolAndSupplyPositions<AccountId, Amount> {
	#[serde(flatten)]
	pub asset: Asset,
	pub positions: Vec<LendingSupplyPosition<AccountId, Amount>>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLiquidationSwap {
	pub swap_request_id: SwapRequestId,
	pub loan_id: LoanId,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLiquidationStatus {
	pub liquidation_swaps: Vec<RpcLiquidationSwap>,
	pub liquidation_type: LiquidationType,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLoanAccount<AccountId, Amount> {
	pub account: AccountId,
	pub ltv_ratio: Option<FixedU64>,
	pub collateral: Vec<AssetAndAmount<Amount>>,
	pub loans: Vec<RpcLoan<AccountId, Amount>>,
	pub liquidation_status: Option<RpcLiquidationStatus>,
}

impl<AccountId> From<RpcLoan<AccountId, AssetAmount>> for RpcLoan<AccountId, U256> {
	fn from(loan: RpcLoan<AccountId, AssetAmount>) -> Self {
		Self {
			loan_id: loan.loan_id,
			loan_type: loan.loan_type,
			asset: loan.asset,
			created_at: loan.created_at,
			principal_amount: loan.principal_amount.into(),
			broker: loan.broker,
		}
	}
}

impl<AccountId> From<RpcLoanAccount<AccountId, AssetAmount>> for RpcLoanAccount<AccountId, U256> {
	fn from(acc: RpcLoanAccount<AccountId, AssetAmount>) -> Self {
		Self {
			account: acc.account,
			ltv_ratio: acc.ltv_ratio,
			collateral: acc.collateral.into_iter().map(Into::into).collect(),
			loans: acc.loans.into_iter().map(Into::into).collect(),
			liquidation_status: acc.liquidation_status,
		}
	}
}

fn build_rpc_loan_account<T: Config>(
	borrower_id: T::AccountId,
	loan_account: LoanAccount<T>,
	price_cache: &OraclePriceCache<T>,
) -> RpcLoanAccount<T::AccountId, AssetAmount> {
	let mut loans = loan_account.loans.clone();

	// Accounting for any partially executed liquidation swaps
	// when reporting on the outstanding principal amount:
	if let LiquidationStatus::Liquidating { liquidation_swaps, .. } =
		&loan_account.liquidation_status
	{
		for (swap_request_id, LiquidationSwap { loan_id, .. }) in liquidation_swaps {
			if let Some(swap_progress) =
				T::SwapRequestHandler::inspect_swap_request(*swap_request_id)
			{
				if let Some(loan) = loans.get_mut(loan_id) {
					loan.owed_principal.saturating_reduce(swap_progress.accumulated_output_amount);
				}
			} else {
				log_or_panic!("Failed to inspect swap request: {swap_request_id}");
			}
		}
	}

	RpcLoanAccount {
		account: borrower_id.clone(),
		ltv_ratio: loan_account.derive_ltv(price_cache).ok(),
		collateral: loan_account
			.get_total_collateral()
			.into_iter()
			.map(|(asset, amount)| AssetAndAmount { asset, amount })
			.collect(),
		loans: loans
			.into_iter()
			.map(|(loan_id, loan)| RpcLoan {
				loan_id,
				loan_type: LoanType::User(borrower_id.clone()),
				asset: loan.asset,
				created_at: loan.created_at_block.unique_saturated_into(),
				principal_amount: loan.owed_principal,
				broker: loan.broker,
			})
			.collect(),
		liquidation_status: match loan_account.liquidation_status {
			LiquidationStatus::NoLiquidation => None,
			LiquidationStatus::Liquidating { liquidation_swaps, liquidation_type } =>
				Some(RpcLiquidationStatus {
					liquidation_swaps: liquidation_swaps
						.into_iter()
						.map(|(swap_request_id, swap)| RpcLiquidationSwap {
							swap_request_id,
							loan_id: swap.loan_id,
						})
						.collect(),
					liquidation_type,
				}),
		},
	}
}

pub fn get_loan_accounts<T: Config>(
	borrower_id: Option<T::AccountId>,
) -> Vec<RpcLoanAccount<T::AccountId, AssetAmount>> {
	let price_cache = OraclePriceCache::<T>::default();

	if let Some(borrower_id) = borrower_id {
		LoanAccounts::<T>::get(&borrower_id)
			.into_iter()
			.map(|loan_account| {
				build_rpc_loan_account(borrower_id.clone(), loan_account, &price_cache)
			})
			.collect()
	} else {
		LoanAccounts::<T>::iter()
			.map(|(borrower_id, loan_account)| {
				build_rpc_loan_account(borrower_id.clone(), loan_account, &price_cache)
			})
			.collect()
	}
}

fn build_rpc_lending_pool<T: Config>(
	asset: Asset,
	pool: &LendingPool<T::AccountId>,
	price_cache: &OraclePriceCache<T>,
) -> RpcLendingPool<AssetAmount> {
	let config = LendingConfig::<T>::get();

	let utilisation = pool.get_utilisation();

	// Total interest/borrow rate is the sum of "base" rate plus "network" rate:
	let current_interest_rate = config.derive_interest_rate_per_year(asset, utilisation) +
		config.network_fee_contributions.extra_interest;

	// Report the cap as `Permill::one()` when it can't be computed (e.g. a missing oracle price
	// for a collateral asset) so the RPC stays informative rather than failing.
	let utilisation_cap =
		compute_utilisation_cap::<T>(asset, config.liquidation_coverage_factor, price_cache)
			.unwrap_or(Permill::one());

	RpcLendingPool {
		asset,
		total_amount: pool.total_amount,
		available_amount: pool.available_amount,
		utilisation_rate: utilisation,
		utilisation_cap,
		current_interest_rate,
		config: config.get_config_for_asset(asset).clone(),
	}
}

pub fn get_all_loans<T: Config>() -> Vec<RpcLoan<T::AccountId, AssetAmount>> {
	let boost_loans =
		BoostedDeposits::<T>::iter().filter_map(|(_, deposit_id, boosted_deposit)| {
			let loan_id = boosted_deposit.lending_loan_id?;
			let loan = BoostLoans::<T>::get(loan_id)?;
			Some(RpcLoan {
				loan_id: loan.id,
				loan_type: LoanType::Boost(deposit_id),
				asset: loan.asset,
				created_at: loan.created_at_block.unique_saturated_into(),
				principal_amount: loan.owed_principal,
				broker: loan.broker,
			})
		});

	let user_loans = LoanAccounts::<T>::iter().flat_map(|(borrower_id, loan_account)| {
		loan_account.loans.into_values().map(move |loan| RpcLoan {
			loan_id: loan.id,
			loan_type: LoanType::User(borrower_id.clone()),
			asset: loan.asset,
			created_at: loan.created_at_block.unique_saturated_into(),
			principal_amount: loan.owed_principal,
			broker: loan.broker,
		})
	});

	boost_loans.chain(user_loans).collect()
}

pub fn get_lending_pools<T: Config>(asset: Option<Asset>) -> Vec<RpcLendingPool<AssetAmount>> {
	let price_cache = OraclePriceCache::<T>::default();

	if let Some(asset) = asset {
		GeneralLendingPools::<T>::get(asset)
			.iter()
			.map(|pool| build_rpc_lending_pool::<T>(asset, pool, &price_cache))
			.collect()
	} else {
		GeneralLendingPools::<T>::iter()
			.map(|(asset, pool)| build_rpc_lending_pool::<T>(asset, &pool, &price_cache))
			.collect()
	}
}
