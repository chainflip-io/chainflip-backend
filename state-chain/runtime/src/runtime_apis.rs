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

use crate::{chainflip::Offence, Runtime, RuntimeSafeMode};
use pallet_cf_elections::electoral_systems::oracle_price::chainlink::OraclePrice;

use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	self, address::EncodedAddress, assets::any::AssetMap, eth::Address as EthereumAddress,
	sol::SolInstructionRpc, CcmChannelMetadataUnchecked, Chain, ChainCrypto,
	ChannelRefundParametersUncheckedEncoded, ForeignChainAddress, VaultSwapExtraParametersEncoded,
	VaultSwapInputEncoded,
};
use cf_primitives::{
	AccountRole, Affiliates, Asset, AssetAmount, BasisPoints, BlockNumber, BroadcastId, ChannelId,
	DcaParameters, EpochIndex, FlipBalance, ForeignChain, GasAmount, NetworkEnvironment, SemVer,
};
use cf_traits::SwapLimits;
use codec::{Decode, Encode};
use core::{ops::Range, str};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_elections::electoral_systems::oracle_price::price::PriceAsset;
use pallet_cf_governance::GovCallHash;
pub use pallet_cf_ingress_egress::ChannelAction;
pub use pallet_cf_lending_pools::BoostPoolDetails;
use pallet_cf_pools::{
	AskBidMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_swapping::{AffiliateDetails, FeeRateAndMinimum, SwapLegInfo};
use pallet_cf_trading_strategy::TradingStrategy;
use pallet_cf_validator::OperatorSettings;
use pallet_cf_witnesser::CallHash;
use scale_info::{prelude::string::String, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_runtime::{DispatchError, Permill};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

type VanityName = Vec<u8>;

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize)]
#[serde(tag = "chain")]
pub enum VaultSwapDetails<BtcAddress> {
	Bitcoin {
		#[serde(with = "sp_core::bytes")]
		nulldata_payload: Vec<u8>,
		deposit_address: BtcAddress,
	},
	Ethereum {
		#[serde(flatten)]
		details: EvmVaultSwapDetails,
	},
	Arbitrum {
		#[serde(flatten)]
		details: EvmVaultSwapDetails,
	},
	Solana {
		#[serde(flatten)]
		instruction: SolInstructionRpc,
	},
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub struct EvmVaultSwapDetails {
	/// The encoded calldata payload including function selector.
	#[serde(with = "sp_core::bytes")]
	pub calldata: Vec<u8>,
	/// The ETH/ArbETH amount. Always 0 for ERC-20 tokens.
	pub value: sp_core::U256,
	/// The vault address for either Ethereum or Arbitrum.
	pub to: sp_core::H160,
	/// The address of the source token that requires user approval for the swap to succeed, if
	/// any.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_token_address: Option<sp_core::H160>,
}

impl<BtcAddress> VaultSwapDetails<BtcAddress> {
	pub fn ethereum(details: EvmVaultSwapDetails) -> Self {
		VaultSwapDetails::Ethereum { details }
	}

	pub fn arbitrum(details: EvmVaultSwapDetails) -> Self {
		VaultSwapDetails::Arbitrum { details }
	}

	pub fn map_btc_address<F, T>(self, f: F) -> VaultSwapDetails<T>
	where
		F: FnOnce(BtcAddress) -> T,
	{
		match self {
			VaultSwapDetails::Bitcoin { nulldata_payload, deposit_address } =>
				VaultSwapDetails::Bitcoin { nulldata_payload, deposit_address: f(deposit_address) },
			VaultSwapDetails::Solana { instruction } => VaultSwapDetails::Solana { instruction },
			VaultSwapDetails::Ethereum { details } => VaultSwapDetails::Ethereum { details },
			VaultSwapDetails::Arbitrum { details } => VaultSwapDetails::Arbitrum { details },
		}
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy, TypeInfo, Serialize, Deserialize)]
pub enum BackupOrPassive {
	Backup,
	Passive,
}

// TEMP: so frontend doesn't break after removal of passive from backend
#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy, TypeInfo, Serialize, Deserialize)]
pub enum ChainflipAccountStateWithPassive {
	CurrentAuthority,
	BackupOrPassive(BackupOrPassive),
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct ValidatorInfo {
	pub balance: AssetAmount,
	pub bond: AssetAmount,
	pub last_heartbeat: u32, // can *maybe* remove this - check with Andrew
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<EpochIndex>,
	pub is_current_authority: bool,
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EthereumAddress>,
	pub apy_bp: Option<u32>, // APY for validator/back only. In Basis points.
	pub restricted_balances: BTreeMap<EthereumAddress, AssetAmount>,
	pub estimated_redeemable_balance: AssetAmount,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct OperatorInfo<Amount> {
	pub managed_validators: BTreeMap<AccountId32, Amount>,
	pub settings: OperatorSettings,
	#[cfg_attr(feature = "std", serde(skip_serializing_if = "Vec::is_empty"))]
	pub allowed: Vec<AccountId32>,
	#[cfg_attr(feature = "std", serde(skip_serializing_if = "Vec::is_empty"))]
	pub blocked: Vec<AccountId32>,
	pub delegators: BTreeMap<AccountId32, Amount>,
}

impl<A> OperatorInfo<A> {
	pub fn map_amounts<F, B>(self, f: F) -> OperatorInfo<B>
	where
		F: Fn(A) -> B,
	{
		OperatorInfo {
			managed_validators: self
				.managed_validators
				.into_iter()
				.map(|(k, v)| (k, f(v)))
				.collect(),
			settings: self.settings,
			allowed: self.allowed,
			blocked: self.blocked,
			delegators: self.delegators.into_iter().map(|(k, v)| (k, f(v))).collect(),
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Clone)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BoostPoolDepth {
	#[cfg_attr(feature = "std", serde(flatten))]
	pub asset: Asset,
	pub tier: u16,
	#[cfg_attr(feature = "std", serde(serialize_with = "serialize_as_hex"))]
	pub available_amount: AssetAmount,
}

#[derive(Encode, Decode, TypeInfo)]
pub enum SimulateSwapAdditionalOrder {
	LimitOrder {
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		tick: Tick,
		sell_amount: AssetAmount,
	},
}

#[cfg(feature = "std")]
fn serialize_as_hex<S>(amount: &AssetAmount, s: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	sp_core::U256::from(*amount).serialize(s)
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RuntimeApiPenalty {
	pub reputation_points: i32,
	pub suspension_duration_blocks: u32,
}

mod old {
	use super::*;

	#[deprecated(note = "Use the new AuctionState struct instead. Remove this after 1.10 release.")]
	#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
	pub struct AuctionState {
		pub epoch_duration: u32,
		pub current_epoch_started_at: u32,
		pub redemption_period_as_percentage: u8,
		pub min_funding: u128,
		pub auction_size_range: (u32, u32),
		pub min_active_bid: Option<u128>,
	}
}

impl From<old::AuctionState> for AuctionState {
	fn from(old: old::AuctionState) -> Self {
		AuctionState {
			epoch_duration: old.epoch_duration,
			current_epoch_started_at: old.current_epoch_started_at,
			redemption_period_as_percentage: old.redemption_period_as_percentage,
			min_funding: old.min_funding,
			min_bid: 0, // min_bid was added in version 5
			auction_size_range: old.auction_size_range,
			min_active_bid: old.min_active_bid,
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct AuctionState {
	pub epoch_duration: u32,
	pub current_epoch_started_at: u32,
	pub redemption_period_as_percentage: u8,
	pub min_funding: u128,
	pub min_bid: u128,
	pub auction_size_range: (u32, u32),
	pub min_active_bid: Option<u128>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct LiquidityProviderBoostPoolInfo {
	pub fee_tier: u16,
	pub total_balance: AssetAmount,
	pub available_balance: AssetAmount,
	pub in_use_balance: AssetAmount,
	pub is_withdrawing: bool,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
	pub earned_fees: AssetMap<AssetAmount>,
	pub boost_balances: AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
}

#[derive(Encode, Decode, TypeInfo, Default)]
pub struct BrokerInfo {
	pub earned_fees: Vec<(Asset, AssetAmount)>,
	pub btc_vault_deposit_address: Option<String>,
	pub affiliates: Vec<(AccountId32, AffiliateDetails)>,
	pub bond: AssetAmount,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct BrokerInfoLegacy {
	pub earned_fees: Vec<(Asset, AssetAmount)>,
}

impl From<BrokerInfoLegacy> for BrokerInfo {
	fn from(legacy: BrokerInfoLegacy) -> Self {
		BrokerInfo { earned_fees: legacy.earned_fees, ..Default::default() }
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct CcmData {
	pub gas_budget: GasAmount,
	pub message_length: u32,
}

#[derive(Encode, Decode, Eq, PartialEq, Ord, PartialOrd, TypeInfo, Serialize, Deserialize)]
pub enum FeeTypes {
	Network,
	IngressDepositChannel,
	Egress,
	IngressVaultSwap,
}

/// Struct that represents the estimated output of a Swap.
#[derive(Encode, Decode, TypeInfo)]
pub struct SimulatedSwapInformation {
	pub intermediary: Option<AssetAmount>,
	pub output: AssetAmount,
	pub network_fee: AssetAmount,
	pub ingress_fee: AssetAmount,
	pub egress_fee: AssetAmount,
	pub broker_fee: AssetAmount,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum DispatchErrorWithMessage {
	Module(Vec<u8>),
	RawMessage(Vec<u8>),
	Other(DispatchError),
}
impl<E: Into<DispatchError>> From<E> for DispatchErrorWithMessage {
	fn from(error: E) -> Self {
		match error.into() {
			DispatchError::Module(sp_runtime::ModuleError { message: Some(message), .. }) =>
				DispatchErrorWithMessage::Module(message.as_bytes().to_vec()),
			DispatchError::Other(message) =>
				DispatchErrorWithMessage::RawMessage(message.as_bytes().to_vec()),
			error => DispatchErrorWithMessage::Other(error),
		}
	}
}

#[cfg(feature = "std")]
impl core::fmt::Display for DispatchErrorWithMessage {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		match self {
			DispatchErrorWithMessage::Module(message) |
			DispatchErrorWithMessage::RawMessage(message) => write!(
				f,
				"{}",
				str::from_utf8(message).unwrap_or("<Error message is not valid UTF-8>")
			),
			DispatchErrorWithMessage::Other(error) => write!(f, "{:?}", error),
		}
	}
}
#[cfg(feature = "std")]
impl std::error::Error for DispatchErrorWithMessage {}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct FailingWitnessValidators {
	pub failing_count: u32,
	pub validators: Vec<(cf_primitives::AccountId, String, bool)>,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct ChainAccounts {
	pub chain_accounts: Vec<EncodedAddress>,
}

#[derive(
	Serialize,
	Deserialize,
	Encode,
	Decode,
	Eq,
	PartialEq,
	TypeInfo,
	Debug,
	Clone,
	Copy,
	PartialOrd,
	Ord,
)]
pub enum ChannelActionType {
	Swap,
	LiquidityProvision,
	Refund,
}

impl<AccountId, C: Chain> From<ChannelAction<AccountId, C>> for ChannelActionType {
	fn from(action: ChannelAction<AccountId, C>) -> Self {
		match action {
			ChannelAction::Swap { .. } => ChannelActionType::Swap,
			ChannelAction::LiquidityProvision { .. } => ChannelActionType::LiquidityProvision,
			ChannelAction::Refund { .. } => ChannelActionType::Refund,
		}
	}
}

pub type OpenedDepositChannels = (AccountId32, ChannelActionType, ChainAccounts);

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub enum TransactionScreeningEvent<TxId> {
	TransactionRejectionRequestReceived {
		account_id: <Runtime as frame_system::Config>::AccountId,
		tx_id: TxId,
	},

	TransactionRejectionRequestExpired {
		account_id: <Runtime as frame_system::Config>::AccountId,
		tx_id: TxId,
	},

	TransactionRejectedByBroker {
		refund_broadcast_id: BroadcastId,
		tx_id: TxId,
	},
}

pub type BrokerRejectionEventFor<C> =
	TransactionScreeningEvent<<<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId>;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct TransactionScreeningEvents {
	pub btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>>,
	pub eth_events: Vec<BrokerRejectionEventFor<cf_chains::Ethereum>>,
	pub arb_events: Vec<BrokerRejectionEventFor<cf_chains::Arbitrum>>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct TradingStrategyInfo<Amount> {
	pub lp_id: AccountId32,
	pub strategy_id: AccountId32,
	pub strategy: TradingStrategy,
	pub balance: Vec<(Asset, Amount)>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct TradingStrategyLimits {
	pub minimum_deployment_amount: AssetMap<Option<AssetAmount>>,
	pub minimum_added_funds_amount: AssetMap<Option<AssetAmount>>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct NetworkFeeDetails {
	pub standard_rate_and_minimum: FeeRateAndMinimum,
	pub rates: AssetMap<Permill>,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct NetworkFees {
	pub regular_network_fee: NetworkFeeDetails,
	pub internal_swap_network_fee: NetworkFeeDetails,
}

// READ THIS BEFORE UPDATING THIS TRAIT:
//
// ## When changing an existing method:
//  - Bump the api_version of the trait, for example from #[api_version(2)] to #[api_version(3)].
//  - Annotate the old method with #[changed_in($VERSION)] where $VERSION is the *new* api_version,
//    for example #[changed_in(3)].
//  - Handle the old method in the custom rpc implementation using runtime_api().api_version().
//
// ## When adding a new method:
//  - Bump the api_version of the trait, for example from #[api_version(2)] to #[api_version(3)].
//  - Create a dummy method with the same name, but no args and no return value.
//  - Annotate the dummy method with #[changed_in($VERSION)] where $VERSION is the *new*
//    api_version.
//  - Handle the dummy method gracefully in the custom rpc implementation using
//    runtime_api().api_version().
decl_runtime_apis!(
	#[api_version(5)]
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn cf_is_auction_phase() -> bool;
		fn cf_eth_flip_token_address() -> EthereumAddress;
		fn cf_eth_state_chain_gateway_address() -> EthereumAddress;
		fn cf_eth_key_manager_address() -> EthereumAddress;
		fn cf_eth_chain_id() -> u64;
		/// Returns the eth vault in the form [agg_key, active_from_eth_block]
		fn cf_eth_vault() -> ([u8; 33], u32);
		/// Returns the Auction params in the form [min_set_size, max_set_size]
		fn cf_auction_parameters() -> (u32, u32);
		fn cf_min_funding() -> u128;
		fn cf_current_epoch() -> u32;
		#[deprecated(note = "Use direct storage access of `CurrentReleaseVersion` instead.")]
		fn cf_current_compatibility_version() -> SemVer;
		fn cf_epoch_duration() -> u32;
		fn cf_current_epoch_started_at() -> u32;
		fn cf_authority_emission_per_block() -> u128;
		fn cf_backup_emission_per_block() -> u128;
		/// Returns the flip supply in the form [total_issuance, offchain_funds]
		fn cf_flip_supply() -> (u128, u128);
		fn cf_accounts() -> Vec<(AccountId32, VanityName)>;
		fn cf_account_flip_balance(account_id: &AccountId32) -> u128;
		fn cf_validator_info(account_id: &AccountId32) -> ValidatorInfo;
		fn cf_operator_info(account_id: &AccountId32) -> OperatorInfo<FlipBalance>;
		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)>;
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId32)>)>;
		fn cf_generate_gov_key_call_hash(call: Vec<u8>) -> GovCallHash;
		#[changed_in(5)]
		fn cf_auction_state() -> old::AuctionState;
		fn cf_auction_state() -> AuctionState;
		fn cf_pool_price(from: Asset, to: Asset) -> Option<PoolPriceV1>;
		fn cf_pool_price_v2(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolPriceV2, DispatchErrorWithMessage>;
		#[changed_in(3)]
		fn cf_pool_simulate_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			broker_commission: BasisPoints,
			dca_parameters: Option<DcaParameters>,
			additional_limit_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage>;
		fn cf_pool_simulate_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			broker_commission: BasisPoints,
			dca_parameters: Option<DcaParameters>,
			ccm_data: Option<CcmData>,
			exclude_fees: BTreeSet<FeeTypes>,
			additional_limit_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
			is_internal: Option<bool>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage>;
		fn cf_pool_info(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolInfo, DispatchErrorWithMessage>;
		fn cf_pool_depth(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
		) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage>;
		fn cf_pool_liquidity(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolLiquidity, DispatchErrorWithMessage>;
		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage>;
		fn cf_pool_orderbook(
			base_asset: Asset,
			quote_asset: Asset,
			orders: u32,
		) -> Result<PoolOrderbook, DispatchErrorWithMessage>;
		fn cf_pool_orders(
			base_asset: Asset,
			quote_asset: Asset,
			lp: Option<AccountId32>,
			filled_orders: bool,
		) -> Result<PoolOrders<Runtime>, DispatchErrorWithMessage>;
		fn cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage>;

		fn cf_max_swap_amount(asset: Asset) -> Option<AssetAmount>;
		fn cf_min_deposit_amount(asset: Asset) -> AssetAmount;
		fn cf_egress_dust_limit(asset: Asset) -> AssetAmount;
		fn cf_scheduled_swaps(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Vec<(SwapLegInfo, BlockNumber)>;
		fn cf_liquidity_provider_info(account_id: AccountId32) -> LiquidityProviderInfo;
		#[changed_in(3)]
		fn cf_broker_info(account_id: AccountId32) -> BrokerInfoLegacy;
		fn cf_broker_info(account_id: AccountId32) -> BrokerInfo;
		fn cf_account_role(account_id: AccountId32) -> Option<AccountRole>;
		fn cf_free_balances(account_id: AccountId32) -> AssetMap<AssetAmount>;
		fn cf_lp_total_balances(account_id: AccountId32) -> AssetMap<AssetAmount>;
		fn cf_redemption_tax() -> AssetAmount;
		fn cf_network_environment() -> NetworkEnvironment;
		fn cf_failed_call_ethereum(
			broadcast_id: BroadcastId,
		) -> Option<<cf_chains::Ethereum as Chain>::Transaction>;
		fn cf_failed_call_arbitrum(
			broadcast_id: BroadcastId,
		) -> Option<<cf_chains::Arbitrum as Chain>::Transaction>;
		fn cf_ingress_fee(asset: Asset) -> Option<AssetAmount>;
		fn cf_egress_fee(asset: Asset) -> Option<AssetAmount>;
		fn cf_witness_count(
			hash: CallHash,
			epoch_index: Option<EpochIndex>,
		) -> Option<FailingWitnessValidators>;
		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64>;
		fn cf_channel_opening_fee(chain: ForeignChain) -> FlipBalance;
		fn cf_boost_pools_depth() -> Vec<BoostPoolDepth>;
		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails<AccountId32>>;
		fn cf_safe_mode_statuses() -> RuntimeSafeMode;
		fn cf_pools() -> Vec<PoolPairsMap<Asset>>;
		fn cf_swap_retry_delay_blocks() -> u32;
		fn cf_swap_limits() -> SwapLimits;
		fn cf_lp_events() -> Vec<pallet_cf_pools::Event<Runtime>>;
		fn cf_minimum_chunk_size(asset: Asset) -> AssetAmount;
		fn cf_validate_dca_params(
			number_of_chunks: u32,
			chunk_interval: u32,
		) -> Result<(), DispatchErrorWithMessage>;
		fn cf_validate_refund_params(
			retry_duration: BlockNumber,
		) -> Result<(), DispatchErrorWithMessage>;
		fn cf_request_swap_parameter_encoding(
			broker: AccountId32,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			extra_parameters: VaultSwapExtraParametersEncoded,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<AccountId32>,
			dca_parameters: Option<DcaParameters>,
		) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage>;
		fn cf_decode_vault_swap_parameter(
			broker: AccountId32,
			vault_swap: VaultSwapDetails<String>,
		) -> Result<VaultSwapInputEncoded, DispatchErrorWithMessage>;
		fn cf_encode_cf_parameters(
			broker: AccountId32,
			source_asset: Asset,
			destination_address: EncodedAddress,
			destination_asset: Asset,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
			dca_parameters: Option<DcaParameters>,
			boost_fee: BasisPoints,
			broker_commission: BasisPoints,
			affiliate_fees: Affiliates<AccountId32>,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
		) -> Result<Vec<u8>, DispatchErrorWithMessage>;
		fn cf_get_open_deposit_channels(account_id: Option<AccountId32>) -> ChainAccounts;
		fn cf_get_preallocated_deposit_channels(
			account_id: AccountId32,
			chain: ForeignChain,
		) -> Vec<ChannelId>;
		fn cf_transaction_screening_events() -> TransactionScreeningEvents;
		fn cf_affiliate_details(
			broker: AccountId32,
			affiliate: Option<AccountId32>,
		) -> Vec<(AccountId32, AffiliateDetails)>;
		fn cf_vault_addresses() -> VaultAddresses;
		fn cf_all_open_deposit_channels() -> Vec<OpenedDepositChannels>;
		fn cf_get_trading_strategies(
			lp_id: Option<AccountId32>,
		) -> Vec<TradingStrategyInfo<AssetAmount>>;
		fn cf_trading_strategy_limits() -> TradingStrategyLimits;
		fn cf_network_fees() -> NetworkFees;
		fn cf_oracle_prices(
			base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,
		) -> Vec<OraclePrice>;
	}
);

decl_runtime_apis!(
	/// Versioning of runtime apis is explained here:
	/// https://docs.rs/sp-api/latest/sp_api/macro.decl_runtime_apis.html
	/// Of course it doesn't explain everything, e.g. there's a very useful
	/// `#[renamed($OLD_NAME, $VERSION)]` attribute which will handle renaming
	/// of apis automatically.
	#[api_version(2)]
	pub trait ElectoralRuntimeApi {
		/// Returns SCALE encoded `Option<ElectoralDataFor<state_chain_runtime::Runtime,
		/// Instance>>`
		#[renamed("cf_electoral_data", 2)]
		fn cf_solana_electoral_data(account_id: AccountId32) -> Vec<u8>;

		/// Returns SCALE encoded `BTreeSet<ElectionIdentifierOf<<state_chain_runtime::Runtime as
		/// pallet_cf_elections::Config<Instance>>::ElectoralSystem>>`
		#[renamed("cf_filter_votes", 2)]
		fn cf_solana_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_bitcoin_electoral_data(account_id: AccountId32) -> Vec<u8>;

		fn cf_bitcoin_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;

		fn cf_generic_electoral_data(account_id: AccountId32) -> Vec<u8>;

		fn cf_generic_filter_votes(account_id: AccountId32, proposed_votes: Vec<u8>) -> Vec<u8>;
	}
);
