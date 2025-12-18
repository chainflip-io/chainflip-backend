use super::*;
use cf_primitives::{AssetAndAmount, SwapRequestId};
use cf_traits::lending::LoanId;
use serde::{Deserialize, Serialize};

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLoan<Amount> {
	pub loan_id: LoanId,
	pub asset: Asset,
	pub created_at: u32,
	pub principal_amount: Amount,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcLendingPool<Amount> {
	pub asset: Asset,
	/// Total amount collectively owed to lenders
	pub total_amount: Amount,
	/// The amount available for borrowing. Could be larger than `total_amount` in a rare edge case
	/// where `total_owed_to_network` is not 0 despite all loans having been fully repaid (in
	/// which case `available_amount` == `total_amount` + `total_owed_to_network`).
	pub available_amount: Amount,
	/// Amount owed to network due to network fees. Expected to be 0 most of the time except when
	/// pool's utilisation is 100% and the network was unable to collect the fees immediately. The
	/// network is expected to collect the fees when `available_amount` becomes > 0.
	pub owed_to_network: Amount,
	pub utilisation_rate: Permill,
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
	pub collateral_topup_asset: Option<Asset>,
	pub ltv_ratio: Option<FixedU64>,
	pub collateral: Vec<AssetAndAmount<Amount>>,
	pub loans: Vec<RpcLoan<Amount>>,
	pub liquidation_status: Option<RpcLiquidationStatus>,
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
		account: borrower_id,
		collateral_topup_asset: loan_account.collateral_topup_asset,
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
				asset: loan.asset,
				created_at: loan.created_at_block.unique_saturated_into(),
				principal_amount: loan.owed_principal,
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
) -> RpcLendingPool<AssetAmount> {
	let config = LendingConfig::<T>::get();

	let utilisation = pool.get_utilisation();

	let current_interest_rate = config.derive_interest_rate_per_year(asset, utilisation);

	RpcLendingPool {
		asset,
		total_amount: pool.total_amount,
		available_amount: pool.available_amount,
		owed_to_network: pool.owed_to_network,
		utilisation_rate: utilisation,
		current_interest_rate,
		config: config.get_config_for_asset(asset).clone(),
	}
}

pub fn get_lending_pools<T: Config>(asset: Option<Asset>) -> Vec<RpcLendingPool<AssetAmount>> {
	if let Some(asset) = asset {
		GeneralLendingPools::<T>::get(asset)
			.iter()
			.map(|pool| build_rpc_lending_pool::<T>(asset, pool))
			.collect()
	} else {
		GeneralLendingPools::<T>::iter()
			.map(|(asset, pool)| build_rpc_lending_pool::<T>(asset, &pool))
			.collect()
	}
}
