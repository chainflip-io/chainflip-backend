use super::*;
use codec::{DecodeWithMemTracking, MaxEncodedLen};

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
	pub sol_vault_program: EncodedAddress,
	pub sol_swap_endpoint_program_data_account: EncodedAddress,
	pub usdc_token_mint_pubkey: EncodedAddress,

	pub bitcoin_vault: Option<EncodedAddress>,
	pub solana_sol_vault: Option<EncodedAddress>,
	pub solana_usdc_token_vault_ata: EncodedAddress,
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
			bitcoin_vault: old.bitcoin_vault,
			solana_sol_vault: old.solana_sol_vault,
			solana_usdc_token_vault_ata: old.solana_usdc_token_vault_ata,
			solana_vault_swap_account: old.solana_vault_swap_account,
			predicted_seconds_until_next_vault_rotation: old
				.predicted_seconds_until_next_vault_rotation,
			// Set usdt token pubkey and ata to null addresses
			usdt_token_mint_pubkey: EncodedAddress::Sol([0u8; 32]),
			solana_usdt_token_vault_ata: EncodedAddress::Sol([0u8; 32]),
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
	pub eth: EthAssetMap<T>,
	#[serde(rename = "Polkadot")]
	#[serde(default)]
	pub dot: cf_primitives::chains::assets::dot::AssetMap<T>,
	#[serde(rename = "Bitcoin")]
	#[serde(default)]
	pub btc: cf_primitives::chains::assets::btc::AssetMap<T>,
	#[serde(rename = "Arbitrum")]
	#[serde(default)]
	pub arb: ArbAssetMap<T>,
	#[serde(rename = "Solana")]
	#[serde(default)]
	pub sol: SolAssetMap<T>,
	#[serde(rename = "Assethub")]
	#[serde(default)]
	pub hub: cf_primitives::chains::assets::hub::AssetMap<T>,
}

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
pub struct EthAssetMap<T> {
	#[serde(rename = "ETH")]
	#[serde(default)]
	pub eth: T,
	#[serde(rename = "FLIP")]
	#[serde(default)]
	pub flip: T,
	#[serde(rename = "USDC")]
	#[serde(default)]
	pub usdc: T,
	#[serde(rename = "USDT")]
	#[serde(default)]
	pub usdt: T,
}
impl<T: Default> From<EthAssetMap<T>> for cf_primitives::chains::assets::eth::AssetMap<T> {
	fn from(value: EthAssetMap<T>) -> Self {
		Self {
			eth: value.eth,
			flip: value.flip,
			usdc: value.usdc,
			usdt: value.usdt,
			wbtc: T::default(),
		}
	}
}

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
pub struct ArbAssetMap<T> {
	#[serde(rename = "ETH")]
	#[serde(default)]
	pub eth: T,
	#[serde(rename = "USDC")]
	#[serde(default)]
	pub usdc: T,
}
impl<T: Default> From<ArbAssetMap<T>> for cf_primitives::chains::assets::arb::AssetMap<T> {
	fn from(value: ArbAssetMap<T>) -> Self {
		Self { eth: value.eth, usdc: value.usdc, usdt: T::default() }
	}
}

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
pub struct SolAssetMap<T> {
	#[serde(rename = "SOL")]
	#[serde(default)]
	pub sol: T,
	#[serde(rename = "USDC")]
	#[serde(default)]
	pub usdc: T,
}
impl<T: Default> From<SolAssetMap<T>> for cf_primitives::chains::assets::sol::AssetMap<T> {
	fn from(value: SolAssetMap<T>) -> Self {
		Self { sol: value.sol, usdc: value.usdc, usdt: T::default() }
	}
}

impl<T: Default> From<AssetMap<T>> for cf_primitives::chains::assets::any::AssetMap<T> {
	fn from(value: AssetMap<T>) -> Self {
		Self {
			eth: value.eth.into(),
			dot: value.dot,
			btc: value.btc,
			arb: value.arb.into(),
			sol: value.sol.into(),
			hub: value.hub,
		}
	}
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
