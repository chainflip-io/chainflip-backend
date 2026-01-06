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

pub use crate::{chainflip::Offence, AccountId, Block, Runtime};
use cf_amm::{common::Side, math::Tick};
use cf_chains::{
	self, address::EncodedAddress, assets::any::AssetMap, evm::Address as EvmAddress,
	sol::SolInstructionRpc, Chain, ChainCrypto, ForeignChainAddress,
};
pub use cf_chains::{dot::PolkadotAccountId, sol::SolAddress, ChainEnvironment};
use cf_primitives::{Asset, BroadcastId, EpochIndex, ForeignChain};
pub use cf_primitives::{AssetAmount, BasisPoints};
use codec::{Decode, Encode};
use ethereum_eip712::eip712::TypedData;
pub use frame_support::BoundedVec;
use frame_support::{sp_runtime::AccountId32, DefaultNoBound};
use n_functor::derive_n_functor;
use pallet_cf_environment::{EthEncodingType, SolEncodingType};
pub use pallet_cf_ingress_egress::ChannelAction;
pub use pallet_cf_lending_pools::{
	before_v12, BoostPoolDetails, LendingPoolAndSupplyPositions, LendingSupplyPosition,
	RpcLendingPool, RpcLoanAccount,
};
pub use pallet_cf_pools::{
	AskBidMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_swapping::{AffiliateDetails, FeeRateAndMinimum};
use pallet_cf_trading_strategy::TradingStrategy;
pub use pallet_cf_validator::DelegationSnapshot;
use pallet_cf_validator::OperatorSettings;
use scale_info::{prelude::string::String, TypeInfo};
pub use serde::{Deserialize, Serialize};
use sp_core::U256;
use sp_runtime::{DispatchError, Permill};
pub use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
	str,
};

#[derive(Clone, Serialize, Deserialize, Encode, Decode, TypeInfo)]
pub enum EncodedNonNativeCallGeneric<T> {
	Eip712(T),
	String(String),
}

pub type EncodedNonNativeCall = EncodedNonNativeCallGeneric<TypedData>;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize, TypeInfo)]
pub enum EncodingType {
	Eth(EthEncodingType),
	Sol(SolEncodingType),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize, TypeInfo)]
#[serde(untagged)]
pub enum NonceOrAccount {
	Nonce(u32),
	Account(AccountId32),
}

#[derive(PartialEq, Eq, Encode, Decode, Clone, TypeInfo, Serialize, Deserialize, Debug)]
pub struct LendingPosition<Amount> {
	#[serde(flatten)]
	pub asset: Asset,
	// Total amount owed to the lender
	pub total_amount: Amount,
	// Total amount available to the lender (equals total_amount if the pool has enough liquidity)
	pub available_amount: Amount,
}

pub type VanityName = Vec<u8>;

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
		pub bound_redeem_address: Option<EvmAddress>,
		pub apy_bp: Option<u32>, // APY for validator/back only. In Basis points.
		pub restricted_balances: BTreeMap<EvmAddress, AssetAmount>,
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
	pub bound_redeem_address: Option<EvmAddress>,
	pub apy_bp: Option<u32>, // APY for validator/back only. In Basis points.
	pub restricted_balances: BTreeMap<EvmAddress, AssetAmount>,
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

pub mod before_version_10;
pub mod before_version_15;
pub mod before_version_16;
pub mod before_version_3;

pub mod old {
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

#[expect(deprecated)]
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

pub mod before_version_9 {
	use super::*;

	#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Default)]
	pub struct LiquidityProviderInfo {
		pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
		pub balances: Vec<(Asset, AssetAmount)>,
		pub earned_fees: AssetMap<AssetAmount>,
		pub boost_balances: AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
	}

	impl From<LiquidityProviderInfo> for super::LiquidityProviderInfo {
		fn from(old: LiquidityProviderInfo) -> Self {
			Self {
				refund_addresses: old.refund_addresses,
				balances: old.balances,
				earned_fees: old.earned_fees,
				boost_balances: old.boost_balances,
				..Default::default()
			}
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

#[derive(Encode, Decode, TypeInfo, DefaultNoBound)]
#[derive_n_functor]
pub struct BrokerInfo<BtcAddress> {
	pub earned_fees: Vec<(Asset, AssetAmount)>,
	pub btc_vault_deposit_address: Option<BtcAddress>,
	pub affiliates: Vec<(AccountId32, AffiliateDetails)>,
	pub bond: AssetAmount,
	pub bound_fee_withdrawal_address: Option<EthereumAddress>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct CcmData {
	pub gas_budget: AssetAmount,
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
#[derive(Encode, Decode, TypeInfo, Debug)]
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
	pub chain_accounts: Vec<(EncodedAddress, Asset)>,
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
	Unrefundable,
}

impl<AccountId, C> From<ChannelAction<AccountId, C>> for ChannelActionType {
	fn from(action: ChannelAction<AccountId, C>) -> Self {
		match action {
			ChannelAction::Swap { .. } => ChannelActionType::Swap,
			ChannelAction::LiquidityProvision { .. } => ChannelActionType::LiquidityProvision,
			ChannelAction::Refund { .. } => ChannelActionType::Refund,
			ChannelAction::Unrefundable => ChannelActionType::Unrefundable,
		}
	}
}

pub type OpenedDepositChannels = (AccountId32, ChannelActionType, ChainAccounts);

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub enum TransactionScreeningEvent<TxId, DepositDetails, Address> {
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
		deposit_details: DepositDetails,
	},

	ChannelRejectionRequestReceived {
		account_id: <Runtime as frame_system::Config>::AccountId,
		deposit_address: Address,
	},
}

pub type BrokerRejectionEventFor<C> = TransactionScreeningEvent<
	<<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId,
	<C as Chain>::DepositDetails,
	<C as Chain>::ChainAccount,
>;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct TransactionScreeningEvents {
	pub btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>>,
	pub eth_events: Vec<BrokerRejectionEventFor<cf_chains::Ethereum>>,
	pub arb_events: Vec<BrokerRejectionEventFor<cf_chains::Arbitrum>>,
	pub sol_events: Vec<BrokerRejectionEventFor<cf_chains::Solana>>,
}

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

mod serialize_vanity_name {
	use super::VanityName;
	use serde::{self, Serializer};

	pub fn from_utf8<S>(name: &VanityName, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match core::str::from_utf8(name) {
			Ok(s) => serializer.serialize_str(s),
			Err(_) => serializer.serialize_str("<Invalid UTF-8>"),
		}
	}
}

use pallet_cf_lending_pools::{LtvThresholds, NetworkFeeContributions};

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, Debug)]
pub struct RpcLendingConfig {
	pub ltv_thresholds: LtvThresholds,
	pub network_fee_contributions: NetworkFeeContributions,
	/// Determines how frequently (in blocks) we check if fees should be swapped into the
	/// pools asset
	pub fee_swap_interval_blocks: u32,
	/// Determines how frequently (in blocks) we collect interest payments from loans.
	pub interest_payment_interval_blocks: u32,
	/// Fees collected in some asset will be swapped into the pool's asset once their usd value
	/// reaches this threshold
	pub fee_swap_threshold_usd: U256,
	/// If loan account's owed interest reaches this threshold, it will be taken from the
	/// account's collateral
	pub interest_collection_threshold_usd: U256,
	/// Soft liquidation swaps will use chunks that are equivalent to this amount of USD
	pub soft_liquidation_swap_chunk_size_usd: U256,
	/// Hard liquidation swaps will use chunks that are equivalent to this amount of USD
	pub hard_liquidation_swap_chunk_size_usd: U256,
	/// Soft liquidation will be executed with this oracle slippage limit
	pub soft_liquidation_max_oracle_slippage: BasisPoints,
	/// Hard liquidation will be executed with this oracle slippage limit
	pub hard_liquidation_max_oracle_slippage: BasisPoints,
	/// All fee swaps from lending will be executed with this oracle slippage limit
	pub fee_swap_max_oracle_slippage: BasisPoints,
	/// Minimum equivalent amount of principal that a loan must have at all times.
	pub minimum_loan_amount_usd: U256,
	/// Minimum amount of that can be added to a lending pool. When removing funds, the user
	/// can't leave less than this amount in the pool (they should remove all funds instead).
	pub minimum_supply_amount_usd: U256,
	/// Minimum equivalent amount of principal that can be used to expand or repay an existing
	/// loan.
	pub minimum_update_loan_amount_usd: U256,
	/// Minimum equivalent amount of collateral that can be added or removed from a loan account.
	pub minimum_update_collateral_amount_usd: U256,
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, Default, Debug)]
pub struct RpcAccountInfoCommonItems<Balance> {
	#[serde(skip_serializing_if = "Vec::is_empty")]
	#[serde(serialize_with = "serialize_vanity_name::from_utf8")]
	pub vanity_name: VanityName,
	pub flip_balance: Balance,
	pub asset_balances: cf_chains::assets::any::AssetMap<Balance>,
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

impl<A> RpcAccountInfoCommonItems<A> {
	pub fn try_map_balances<B, E>(
		self,
		f: impl Fn(A) -> Result<B, E>,
	) -> Result<RpcAccountInfoCommonItems<B>, E> {
		Ok(RpcAccountInfoCommonItems {
			vanity_name: self.vanity_name,
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
