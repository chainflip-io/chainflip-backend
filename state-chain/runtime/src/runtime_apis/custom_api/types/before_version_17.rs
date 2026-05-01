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

// VaultAddresses as returned by api_version 16 runtimes: has usdt fields added in v16
// but lacks the tron field added in v17.
#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
	pub sol_vault_program: EncodedAddress,
	pub sol_swap_endpoint_program_data_account: EncodedAddress,
	pub usdc_token_mint_pubkey: EncodedAddress,
	pub usdt_token_mint_pubkey: EncodedAddress,

	pub bitcoin_vault: Option<EncodedAddress>,
	pub solana_sol_vault: Option<EncodedAddress>,
	pub solana_usdc_token_vault_ata: EncodedAddress,
	pub solana_usdt_token_vault_ata: EncodedAddress,
	pub solana_vault_swap_account: Option<EncodedAddress>,

	pub predicted_seconds_until_next_vault_rotation: u64,
}

impl From<VaultAddresses> for super::VaultAddresses {
	fn from(old: VaultAddresses) -> Self {
		Self {
			ethereum: old.ethereum,
			arbitrum: old.arbitrum,
			bitcoin: old.bitcoin,
			sol_vault_program: old.sol_vault_program,
			sol_swap_endpoint_program_data_account: old.sol_swap_endpoint_program_data_account,
			usdc_token_mint_pubkey: old.usdc_token_mint_pubkey,
			usdt_token_mint_pubkey: old.usdt_token_mint_pubkey,
			bitcoin_vault: old.bitcoin_vault,
			solana_sol_vault: old.solana_sol_vault,
			solana_usdc_token_vault_ata: old.solana_usdc_token_vault_ata,
			solana_usdt_token_vault_ata: old.solana_usdt_token_vault_ata,
			solana_vault_swap_account: old.solana_vault_swap_account,
			tron: EncodedAddress::Tron([0u8; 20]),
			predicted_seconds_until_next_vault_rotation: old
				.predicted_seconds_until_next_vault_rotation,
		}
	}
}
