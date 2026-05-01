use super::*;
use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinCryptoInstance, BitcoinInstance, EthereumInstance,
	EvmInstance, PolkadotCryptoInstance, PolkadotInstance, SolanaCryptoInstance, SolanaInstance,
};
use cf_traits::{lending::LoanId, SafeModeSet};
use codec::{DecodeWithMemTracking, MaxEncodedLen};
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

// AssetMap for api_version 16 runtimes: same per-chain fields as v17 (WBTC, ArbUSDT, SolUSDT
// were all added at v16) but without the Tron chain added at v17.
#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Serialize,
	Deserialize,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
#[serde(bound(deserialize = "T: Deserialize<'de> + Default"))]
pub struct AssetMap<T> {
	#[serde(rename = "Ethereum")]
	#[serde(default)]
	pub eth: cf_primitives::chains::assets::eth::AssetMap<T>,
	#[serde(rename = "Polkadot")]
	#[serde(default)]
	pub dot: cf_primitives::chains::assets::dot::AssetMap<T>,
	#[serde(rename = "Bitcoin")]
	#[serde(default)]
	pub btc: cf_primitives::chains::assets::btc::AssetMap<T>,
	#[serde(rename = "Arbitrum")]
	#[serde(default)]
	pub arb: cf_primitives::chains::assets::arb::AssetMap<T>,
	#[serde(rename = "Solana")]
	#[serde(default)]
	pub sol: cf_primitives::chains::assets::sol::AssetMap<T>,
	#[serde(rename = "Assethub")]
	#[serde(default)]
	pub hub: cf_primitives::chains::assets::hub::AssetMap<T>,
}

impl<T: Default> From<AssetMap<T>> for cf_primitives::chains::assets::any::AssetMap<T> {
	fn from(value: AssetMap<T>) -> Self {
		Self {
			eth: value.eth,
			dot: value.dot,
			btc: value.btc,
			arb: value.arb,
			sol: value.sol,
			hub: value.hub,
			tron: Default::default(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct NetworkFeeDetails {
	pub standard_rate_and_minimum: FeeRateAndMinimum,
	pub rates: AssetMap<Permill>,
}

impl From<NetworkFeeDetails> for super::NetworkFeeDetails {
	fn from(value: NetworkFeeDetails) -> Self {
		Self {
			standard_rate_and_minimum: value.standard_rate_and_minimum,
			rates: value.rates.into(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct NetworkFees {
	pub regular_network_fee: NetworkFeeDetails,
	pub internal_swap_network_fee: NetworkFeeDetails,
}

impl From<NetworkFees> for super::NetworkFees {
	fn from(value: NetworkFees) -> Self {
		Self {
			regular_network_fee: value.regular_network_fee.into(),
			internal_swap_network_fee: value.internal_swap_network_fee.into(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct TradingStrategyLimits {
	pub minimum_deployment_amount: AssetMap<Option<AssetAmount>>,
	pub minimum_added_funds_amount: AssetMap<Option<AssetAmount>>,
}

impl From<TradingStrategyLimits> for super::TradingStrategyLimits {
	fn from(value: TradingStrategyLimits) -> Self {
		Self {
			minimum_deployment_amount: value.minimum_deployment_amount.into(),
			minimum_added_funds_amount: value.minimum_added_funds_amount.into(),
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Default)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
	pub earned_fees: AssetMap<AssetAmount>,
	pub boost_balances: AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
	pub lending_positions: Vec<LendingPosition<AssetAmount>>,
	pub collateral_balances: Vec<(Asset, AssetAmount)>,
}

impl From<LiquidityProviderInfo> for super::LiquidityProviderInfo {
	fn from(value: LiquidityProviderInfo) -> Self {
		Self {
			refund_addresses: value.refund_addresses,
			balances: value.balances,
			earned_fees: value.earned_fees.into(),
			boost_balances: value.boost_balances.into(),
			lending_positions: value.lending_positions,
			collateral_balances: value.collateral_balances,
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, Default, Debug)]
#[serde(bound(deserialize = "Balance: Deserialize<'de> + Default"))]
pub struct RpcAccountInfoCommonItems<Balance> {
	#[serde(skip_serializing_if = "Vec::is_empty")]
	#[serde(serialize_with = "serialize_vanity_name::from_utf8")]
	pub vanity_name: VanityName,
	pub flip_balance: Balance,
	pub asset_balances: AssetMap<Balance>,
	pub bond: Balance,
	pub estimated_redeemable_balance: Balance,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bound_redeem_address: Option<EvmAddress>,
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	pub restricted_balances: BTreeMap<EvmAddress, Balance>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub current_delegation_status: Option<DelegationInfo<Balance>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub upcoming_delegation_status: Option<DelegationInfo<Balance>>,
}

impl<B: Default> From<RpcAccountInfoCommonItems<B>> for super::RpcAccountInfoCommonItems<B> {
	fn from(value: RpcAccountInfoCommonItems<B>) -> Self {
		Self {
			vanity_name: value.vanity_name,
			flip_balance: value.flip_balance,
			asset_balances: value.asset_balances.into(),
			bond: value.bond,
			estimated_redeemable_balance: value.estimated_redeemable_balance,
			bound_redeem_address: value.bound_redeem_address,
			restricted_balances: value.restricted_balances,
			current_delegation_status: value.current_delegation_status,
			upcoming_delegation_status: value.upcoming_delegation_status,
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

// The v16 WitnesserCallPermission, without Tron fields added in v17.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Default,
	Copy,
	Clone,
	PartialEq,
	Eq,
	frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct WitnesserCallPermission {
	pub governance: bool,
	pub funding: bool,
	pub swapping: bool,
	pub ethereum_broadcast: bool,
	pub ethereum_chain_tracking: bool,
	pub ethereum_ingress_egress: bool,
	pub ethereum_vault: bool,
	pub polkadot_broadcast: bool,
	pub polkadot_chain_tracking: bool,
	pub polkadot_ingress_egress: bool,
	pub polkadot_vault: bool,
	pub bitcoin_broadcast: bool,
	pub bitcoin_chain_tracking: bool,
	pub bitcoin_ingress_egress: bool,
	pub bitcoin_vault: bool,
	pub arbitrum_broadcast: bool,
	pub arbitrum_chain_tracking: bool,
	pub arbitrum_ingress_egress: bool,
	pub arbitrum_vault: bool,
	pub solana_broadcast: bool,
	pub solana_vault: bool,
	pub assethub_broadcast: bool,
	pub assethub_chain_tracking: bool,
	pub assethub_ingress_egress: bool,
	pub assethub_vault: bool,
}

impl From<WitnesserCallPermission> for crate::safe_mode::WitnesserCallPermission {
	fn from(old: WitnesserCallPermission) -> Self {
		Self {
			governance: old.governance,
			funding: old.funding,
			swapping: old.swapping,
			ethereum_broadcast: old.ethereum_broadcast,
			ethereum_chain_tracking: old.ethereum_chain_tracking,
			ethereum_ingress_egress: old.ethereum_ingress_egress,
			ethereum_vault: old.ethereum_vault,
			polkadot_broadcast: old.polkadot_broadcast,
			polkadot_chain_tracking: old.polkadot_chain_tracking,
			polkadot_ingress_egress: old.polkadot_ingress_egress,
			polkadot_vault: old.polkadot_vault,
			bitcoin_broadcast: old.bitcoin_broadcast,
			bitcoin_chain_tracking: old.bitcoin_chain_tracking,
			bitcoin_ingress_egress: old.bitcoin_ingress_egress,
			bitcoin_vault: old.bitcoin_vault,
			arbitrum_broadcast: old.arbitrum_broadcast,
			arbitrum_chain_tracking: old.arbitrum_chain_tracking,
			arbitrum_ingress_egress: old.arbitrum_ingress_egress,
			arbitrum_vault: old.arbitrum_vault,
			solana_broadcast: old.solana_broadcast,
			solana_vault: old.solana_vault,
			assethub_broadcast: old.assethub_broadcast,
			assethub_chain_tracking: old.assethub_chain_tracking,
			assethub_ingress_egress: old.assethub_ingress_egress,
			assethub_vault: old.assethub_vault,
			tron_broadcast: true,
			tron_chain_tracking: true,
			tron_ingress_egress: true,
			tron_vault: true,
		}
	}
}

// The v16 LendingPoolsSafeMode, with add_collateral/remove_collateral removed in v17.
#[derive(
	Encode, Decode, TypeInfo, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct LendingPoolsSafeMode {
	pub add_boost_funds_enabled: bool,
	pub stop_boosting_enabled: bool,
	pub borrowing: SafeModeSet<Asset>,
	pub add_lender_funds: SafeModeSet<Asset>,
	pub withdraw_lender_funds: SafeModeSet<Asset>,
	pub add_collateral: SafeModeSet<Asset>,
	pub remove_collateral: SafeModeSet<Asset>,
	pub liquidations_enabled: bool,
}

impl From<LendingPoolsSafeMode> for pallet_cf_lending_pools::PalletSafeMode {
	fn from(old: LendingPoolsSafeMode) -> Self {
		Self {
			add_boost_funds_enabled: old.add_boost_funds_enabled,
			stop_boosting_enabled: old.stop_boosting_enabled,
			borrowing: old.borrowing,
			add_lender_funds: old.add_lender_funds,
			withdraw_lender_funds: old.withdraw_lender_funds,
			liquidations_enabled: old.liquidations_enabled,
		}
	}
}

// The v16 RuntimeSafeMode: no broadcast_tron, ingress_egress_tron, or tron_elections fields,
// and with the old LendingPoolsSafeMode and WitnesserCallPermission.
#[derive(
	Encode, Decode, TypeInfo, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct RuntimeSafeMode {
	pub emissions: pallet_cf_emissions::PalletSafeMode,
	pub funding: pallet_cf_funding::PalletSafeMode,
	pub swapping: pallet_cf_swapping::PalletSafeMode,
	pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
	pub validator: pallet_cf_validator::PalletSafeMode,
	pub pools: pallet_cf_pools::PalletSafeMode,
	pub trading_strategies: pallet_cf_trading_strategy::PalletSafeMode,
	pub lending_pools: LendingPoolsSafeMode,
	pub reputation: pallet_cf_reputation::PalletSafeMode,
	pub asset_balances: pallet_cf_asset_balances::PalletSafeMode,
	pub threshold_signature_evm: pallet_cf_threshold_signature::PalletSafeMode<EvmInstance>,
	pub threshold_signature_bitcoin:
		pallet_cf_threshold_signature::PalletSafeMode<BitcoinCryptoInstance>,
	pub threshold_signature_polkadot:
		pallet_cf_threshold_signature::PalletSafeMode<PolkadotCryptoInstance>,
	pub threshold_signature_solana:
		pallet_cf_threshold_signature::PalletSafeMode<SolanaCryptoInstance>,
	pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
	pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
	pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<PolkadotInstance>,
	pub broadcast_arbitrum: pallet_cf_broadcast::PalletSafeMode<ArbitrumInstance>,
	pub broadcast_solana: pallet_cf_broadcast::PalletSafeMode<SolanaInstance>,
	pub broadcast_assethub: pallet_cf_broadcast::PalletSafeMode<AssethubInstance>,
	pub witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
	pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
	pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
	pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance>,
	pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
	pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance>,
	pub ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<AssethubInstance>,
	pub elections_generic:
		crate::chainflip::witnessing::generic_elections::GenericElectionsSafeMode,
	pub ethereum_elections:
		crate::chainflip::witnessing::ethereum_elections::EthereumElectionsSafeMode,
	pub arbitrum_elections:
		crate::chainflip::witnessing::arbitrum_elections::ArbitrumElectionsSafeMode,
}

impl From<RuntimeSafeMode> for crate::safe_mode::RuntimeSafeMode {
	fn from(old: RuntimeSafeMode) -> Self {
		use cf_traits::SafeMode;
		let witnesser = match old.witnesser {
			pallet_cf_witnesser::PalletSafeMode::CodeGreen =>
				pallet_cf_witnesser::PalletSafeMode::CodeGreen,
			pallet_cf_witnesser::PalletSafeMode::CodeRed =>
				pallet_cf_witnesser::PalletSafeMode::CodeRed,
			pallet_cf_witnesser::PalletSafeMode::CodeAmber(old_perms) =>
				pallet_cf_witnesser::PalletSafeMode::CodeAmber(old_perms.into()),
		};
		Self {
			emissions: old.emissions,
			funding: old.funding,
			swapping: old.swapping,
			liquidity_provider: old.liquidity_provider,
			validator: old.validator,
			pools: old.pools,
			trading_strategies: old.trading_strategies,
			lending_pools: old.lending_pools.into(),
			reputation: old.reputation,
			asset_balances: old.asset_balances,
			threshold_signature_evm: old.threshold_signature_evm,
			threshold_signature_bitcoin: old.threshold_signature_bitcoin,
			threshold_signature_polkadot: old.threshold_signature_polkadot,
			threshold_signature_solana: old.threshold_signature_solana,
			broadcast_ethereum: old.broadcast_ethereum,
			broadcast_bitcoin: old.broadcast_bitcoin,
			broadcast_polkadot: old.broadcast_polkadot,
			broadcast_arbitrum: old.broadcast_arbitrum,
			broadcast_solana: old.broadcast_solana,
			broadcast_assethub: old.broadcast_assethub,
			broadcast_tron:
				<pallet_cf_broadcast::PalletSafeMode<_> as SafeMode>::code_green(),
			witnesser,
			ingress_egress_ethereum: old.ingress_egress_ethereum,
			ingress_egress_bitcoin: old.ingress_egress_bitcoin,
			ingress_egress_polkadot: old.ingress_egress_polkadot,
			ingress_egress_arbitrum: old.ingress_egress_arbitrum,
			ingress_egress_solana: old.ingress_egress_solana,
			ingress_egress_assethub: old.ingress_egress_assethub,
			ingress_egress_tron:
				<pallet_cf_ingress_egress::PalletSafeMode<_> as SafeMode>::code_green(),
			elections_generic: old.elections_generic,
			ethereum_elections: old.ethereum_elections,
			arbitrum_elections: old.arbitrum_elections,
			tron_elections:
				<crate::chainflip::witnessing::tron_elections::TronElectionsSafeMode as SafeMode>::code_green(),
		}
	}
}
