use super::*;
use cf_traits::lending::LoanId;
use frame_support::sp_runtime::Percent;
use pallet_cf_lending_pools::LendingPoolConfiguration;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BoostConfiguration {
	pub network_fee_deduction_from_boost_percent: Percent,
	pub minimum_add_funds_amount: BTreeMap<Asset, AssetAmount>,
}

impl From<BoostConfiguration> for super::BoostConfiguration {
	fn from(old: BoostConfiguration) -> Self {
		Self {
			network_fee_deduction_from_boost_percent: old.network_fee_deduction_from_boost_percent,
			minimum_add_funds_amount: old.minimum_add_funds_amount,
			min_lending_pool_share: Percent::from_percent(30),
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct RpcLoan<Amount> {
	pub loan_id: LoanId,
	pub asset: Asset,
	pub created_at: u32,
	pub principal_amount: Amount,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct RpcLoanAccount<AccountId, Amount> {
	pub account: AccountId,
	pub collateral_topup_asset: Option<Asset>,
	pub ltv_ratio: Option<sp_runtime::FixedU64>,
	pub collateral: Vec<cf_primitives::AssetAndAmount<Amount>>,
	pub loans: Vec<RpcLoan<Amount>>,
	pub liquidation_status: Option<RpcLiquidationStatus>,
}

impl<AccountId: Clone> From<RpcLoanAccount<AccountId, AssetAmount>>
	for super::RpcLoanAccount<AccountId, U256>
{
	fn from(acc: RpcLoanAccount<AccountId, AssetAmount>) -> Self {
		let account = acc.account;
		Self {
			account: account.clone(),
			collateral_topup_asset: acc.collateral_topup_asset,
			ltv_ratio: acc.ltv_ratio,
			collateral: acc.collateral.into_iter().map(Into::into).collect(),
			loans: acc
				.loans
				.into_iter()
				.map(|loan| super::RpcLoan {
					loan_id: loan.loan_id,
					asset: loan.asset,
					created_at: loan.created_at,
					loan_type: super::LoanType::User(account.clone()),
					principal_amount: loan.principal_amount.into(),
				})
				.collect(),
			liquidation_status: acc.liquidation_status,
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
pub struct RpcLendingPool<Amount> {
	pub asset: Asset,
	pub total_amount: Amount,
	pub available_amount: Amount,
	pub owed_to_network: Amount,
	pub utilisation_rate: Permill,
	pub current_interest_rate: Permill,
	pub config: LendingPoolConfiguration,
}

impl<Amount> From<RpcLendingPool<Amount>> for pallet_cf_lending_pools::RpcLendingPool<Amount> {
	fn from(value: RpcLendingPool<Amount>) -> Self {
		Self {
			asset: value.asset,
			total_amount: value.total_amount,
			available_amount: value.available_amount,
			owed_to_network: value.owed_to_network,
			utilisation_rate: value.utilisation_rate,
			utilisation_cap: Permill::one(),
			current_interest_rate: value.current_interest_rate,
			config: value.config,
		}
	}
}
