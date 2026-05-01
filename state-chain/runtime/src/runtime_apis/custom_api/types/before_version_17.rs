use super::*;
use cf_traits::lending::LoanId;
use frame_support::sp_runtime::Percent;
use pallet_cf_lending_pools::{LendingPoolConfiguration, NetworkFeeContributions};

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
					// Legacy loans don't have brokers:
					broker: None,
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

#[derive(Encode, Decode, TypeInfo, Clone, Debug)]
pub struct LtvThresholds {
	pub target: Permill,
	pub topup: Option<Permill>,
	pub soft_liquidation: Permill,
	pub soft_liquidation_abort: Permill,
	pub hard_liquidation: Permill,
	pub hard_liquidation_abort: Permill,
	pub low_ltv: Permill,
}

impl From<LtvThresholds> for pallet_cf_lending_pools::LtvThresholds {
	fn from(value: LtvThresholds) -> Self {
		Self {
			target: value.target,
			soft_liquidation: value.soft_liquidation,
			soft_liquidation_abort: value.soft_liquidation_abort,
			hard_liquidation: value.hard_liquidation,
			hard_liquidation_abort: value.hard_liquidation_abort,
			low_ltv: value.low_ltv,
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, Debug)]
pub struct RpcLendingConfig {
	pub ltv_thresholds: LtvThresholds,
	pub network_fee_contributions: NetworkFeeContributions,
	pub fee_swap_interval_blocks: u32,
	pub interest_payment_interval_blocks: u32,
	pub fee_swap_threshold_usd: U256,
	pub interest_collection_threshold_usd: U256,
	pub soft_liquidation_swap_chunk_size_usd: U256,
	pub hard_liquidation_swap_chunk_size_usd: U256,
	pub soft_liquidation_max_oracle_slippage: BasisPoints,
	pub hard_liquidation_max_oracle_slippage: BasisPoints,
	pub fee_swap_max_oracle_slippage: BasisPoints,
	pub minimum_loan_amount_usd: U256,
	pub minimum_supply_amount_usd: U256,
	pub minimum_update_loan_amount_usd: U256,
	pub minimum_update_supply_amount_usd: U256,
}

impl From<RpcLendingConfig> for super::RpcLendingConfig {
	fn from(value: RpcLendingConfig) -> Self {
		Self {
			ltv_thresholds: value.ltv_thresholds.into(),
			network_fee_contributions: value.network_fee_contributions,
			fee_swap_interval_blocks: value.fee_swap_interval_blocks,
			interest_payment_interval_blocks: value.interest_payment_interval_blocks,
			fee_swap_threshold_usd: value.fee_swap_threshold_usd,
			interest_collection_threshold_usd: value.interest_collection_threshold_usd,
			soft_liquidation_swap_chunk_size_usd: value.soft_liquidation_swap_chunk_size_usd,
			hard_liquidation_swap_chunk_size_usd: value.hard_liquidation_swap_chunk_size_usd,
			soft_liquidation_max_oracle_slippage: value.soft_liquidation_max_oracle_slippage,
			hard_liquidation_max_oracle_slippage: value.hard_liquidation_max_oracle_slippage,
			fee_swap_max_oracle_slippage: value.fee_swap_max_oracle_slippage,
			minimum_loan_amount_usd: value.minimum_loan_amount_usd,
			minimum_supply_amount_usd: value.minimum_supply_amount_usd,
			minimum_update_loan_amount_usd: value.minimum_update_loan_amount_usd,
			minimum_update_supply_amount_usd: value.minimum_update_supply_amount_usd,
		}
	}
}
