use super::*;
use cf_traits::lending::LoanId;

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
