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

pub use pallet_cf_validator::DelegationSnapshot;

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
		details: EvmCallDetails,
	},
	Arbitrum {
		#[serde(flatten)]
		details: EvmCallDetails,
	},
	Solana {
		#[serde(flatten)]
		instruction: SolInstructionRpc,
	},
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub struct EvmCallDetails {
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
	pub fn ethereum(details: EvmCallDetails) -> Self {
		VaultSwapDetails::Ethereum { details }
	}

	pub fn arbitrum(details: EvmCallDetails) -> Self {
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

pub mod validator_info_before_v7 {
	use super::*;
	#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
	pub struct ValidatorInfo {
		pub balance: AssetAmount,
		pub bond: AssetAmount,
		pub last_heartbeat: u32, // can *maybe* remove this - check with Andrew
		pub reputation_points: i32,
		pub keyholder_epochs: Vec<EpochIndex>,
		pub is_current_authority: bool,
		#[deprecated]
		pub is_current_backup: bool,
		pub is_qualified: bool,
		pub is_online: bool,
		pub is_bidding: bool,
		pub bound_redeem_address: Option<EthereumAddress>,
		pub apy_bp: Option<u32>, // APY for validator/back only. In Basis points.
		pub restricted_balances: BTreeMap<EthereumAddress, AssetAmount>,
		pub estimated_redeemable_balance: AssetAmount,
	}
}

impl From<validator_info_before_v7::ValidatorInfo> for ValidatorInfo {
	fn from(old: validator_info_before_v7::ValidatorInfo) -> Self {
		ValidatorInfo {
			balance: old.balance,
			bond: old.bond,
			last_heartbeat: old.last_heartbeat,
			reputation_points: old.reputation_points,
			keyholder_epochs: old.keyholder_epochs,
			is_current_authority: old.is_current_authority,
			is_current_backup: old.is_current_backup,
			is_qualified: old.is_qualified,
			is_online: old.is_online,
			is_bidding: old.is_bidding,
			bound_redeem_address: old.bound_redeem_address,
			apy_bp: old.apy_bp,
			restricted_balances: old.restricted_balances,
			estimated_redeemable_balance: old.estimated_redeemable_balance,
			operator: None,
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct ValidatorInfo {
	pub balance: AssetAmount,
	pub bond: AssetAmount,
	pub last_heartbeat: u32, // can *maybe* remove this - check with Andrew
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<EpochIndex>,
	pub is_current_authority: bool,
	#[deprecated]
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EthereumAddress>,
	pub apy_bp: Option<u32>, // APY for validator/back only. In Basis points.
	pub restricted_balances: BTreeMap<EthereumAddress, AssetAmount>,
	pub estimated_redeemable_balance: AssetAmount,
	pub operator: Option<AccountId32>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Clone, Debug, Serialize, Deserialize)]
pub struct OperatorInfo<Amount> {
	pub managed_validators: BTreeMap<AccountId32, Amount>,
	pub settings: OperatorSettings,
	#[cfg_attr(feature = "std", serde(skip_serializing_if = "Vec::is_empty"))]
	pub allowed: Vec<AccountId32>,
	#[cfg_attr(feature = "std", serde(skip_serializing_if = "Vec::is_empty"))]
	pub blocked: Vec<AccountId32>,
	// TODO: ensure max bid is respected.
	pub delegators: BTreeMap<AccountId32, Amount>,
	#[cfg_attr(feature = "std", serde(skip_serializing_if = "Option::is_none"))]
	pub active_delegation: Option<DelegationSnapshot<AccountId32, Amount>>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Clone, Debug, Serialize, Deserialize)]
pub struct DelegationInfo<Amount> {
	pub operator: AccountId32,
	pub bid: Amount,
}

impl<Amount> DelegationInfo<Amount> {
	pub fn map_bid<B>(self, f: impl Fn(Amount) -> B + 'static) -> DelegationInfo<B> {
		DelegationInfo { operator: self.operator, bid: f(self.bid) }
	}
	pub fn try_map_bid<B, E>(
		self,
		f: impl Fn(Amount) -> Result<B, E>,
	) -> Result<DelegationInfo<B>, E> {
		Ok(DelegationInfo { operator: self.operator, bid: f(self.bid)? })
	}
}

impl<A> OperatorInfo<A> {
	pub fn map_amounts<B>(self, f: impl Fn(A) -> B + 'static) -> OperatorInfo<B> {
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
			active_delegation: self.active_delegation.map(|d| d.map_bids(&f)),
		}
	}

	pub fn try_map_amounts<B, E>(
		self,
		f: impl Fn(A) -> Result<B, E> + 'static,
	) -> Result<OperatorInfo<B>, E> {
		Ok(OperatorInfo {
			managed_validators: self
				.managed_validators
				.into_iter()
				.map(|(k, v)| Ok((k, f(v)?)))
				.collect::<Result<_, E>>()?,
			settings: self.settings,
			allowed: self.allowed,
			blocked: self.blocked,
			delegators: self
				.delegators
				.into_iter()
				.map(|(k, v)| Ok((k, f(v)?)))
				.collect::<Result<_, E>>()?,
			active_delegation: self.active_delegation.map(|d| d.try_map_bids(&f)).transpose()?,
		})
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

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, Default, Debug)]
pub struct RpcAccountInfoCommonItems<Balance> {
	pub flip_balance: Balance,
	pub asset_balances: cf_chains::assets::any::AssetMap<Balance>,
	pub bond: Balance,
	pub estimated_redeemable_balance: Balance,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bound_redeem_address: Option<EthereumAddress>,
	#[serde(skip_serializing_if = "BTreeMap::is_empty")]
	pub restricted_balances: BTreeMap<EthereumAddress, Balance>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub current_delegation_status: Option<DelegationInfo<Balance>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub upcoming_delegation_status: Option<DelegationInfo<Balance>>,
}

impl<A> RpcAccountInfoCommonItems<A> {
	pub fn try_map_balances<B, E>(
		self,
		f: impl Fn(A) -> Result<B, E>,
	) -> Result<RpcAccountInfoCommonItems<B>, E> {
		Ok(RpcAccountInfoCommonItems {
			flip_balance: f(self.flip_balance)?,
			asset_balances: self.asset_balances.try_map(&f)?,
			bond: f(self.bond)?,
			estimated_redeemable_balance: f(self.estimated_redeemable_balance)?,
			bound_redeem_address: self.bound_redeem_address,
			restricted_balances: self
				.restricted_balances
				.into_iter()
				.map(|(k, v)| Ok((k, f(v)?)))
				.collect::<Result<_, E>>()?,
			upcoming_delegation_status: self
				.upcoming_delegation_status
				.map(|d| d.try_map_bid(&f))
				.transpose()?,
			current_delegation_status: self
				.current_delegation_status
				.map(|d| d.try_map_bid(&f))
				.transpose()?,
		})
	}
}
