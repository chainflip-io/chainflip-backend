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
#![feature(iterator_try_collect)]
#![feature(duration_constructors)]

use crate::{backend::CustomRpcBackend, boost_pool_rpc::BoostPoolFeesRpc};
use boost_pool_rpc::BoostPoolDetailsRpc;
use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount as AmmAmount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{
		AddressString, EncodedAddress, ForeignChainAddressHumanreadable, ToHumanreadableAddress,
	},
	eth::Address as EthereumAddress,
	instances::{ArbitrumInstance, BitcoinInstance, EthereumInstance},
	CcmChannelMetadataUnchecked, Chain, ChainCrypto, ChannelRefundParametersUnchecked,
	IntoTransactionInIdForAnyChain, MAX_CCM_MSG_LENGTH,
};
use cf_node_client::events_decoder;
use cf_primitives::{
	chains::assets::any::{self, AssetMap},
	AccountRole, Affiliates, Asset, AssetAmount, AssetAndAmount, BasisPoints, BlockNumber,
	BroadcastId, ChannelId, DcaParameters, EpochIndex, ForeignChain, NetworkEnvironment, SemVer,
	SwapId, SwapRequestId,
};
use cf_rpc_apis::{
	broker::{
		try_into_swap_extra_params_encoded, vault_swap_input_encoded_to_rpc, RpcBytes,
		VaultSwapExtraParametersRpc, VaultSwapInputRpc,
	},
	call_error, internal_error, CfErrorCode, NotificationBehaviour, OrderFills,
	RefundParametersRpc, RpcApiError, RpcResult,
};
use cf_utilities::{rpc::NumberOrHex, ArrayCollect};
use core::ops::Range;
use ethereum_eip712::build_eip712_data::to_ethers_typed_data;
use itertools::Itertools;
use jsonrpsee::{
	core::async_trait,
	proc_macros::rpc,
	types::{
		error::{ErrorObject, ErrorObjectOwned},
		ErrorCode,
	},
	PendingSubscriptionSink,
};
use pallet_cf_broadcast::TransactionOutIdToBroadcastId;
use pallet_cf_elections::electoral_systems::oracle_price::{
	chainlink::OraclePrice, price::PriceAsset,
};
use pallet_cf_environment::TransactionMetadata;
use pallet_cf_governance::GovCallHash;
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use pallet_cf_lending_pools::{
	LendingPoolAndSupplyPositions, LendingSupplyPosition, RpcLoan, RpcLoanAccount,
};
use pallet_cf_pools::{
	AskBidMap, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1, UnidirectionalPoolDepth,
};
use pallet_cf_swapping::{AffiliateDetails, SwapLegInfo};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, BlockchainEvents, ExecutorProvider,
	HeaderBackend, StorageProvider,
};
use serde::{Deserialize, Serialize};
use sp_api::{ApiError, ApiExt, CallApiAt};
use sp_core::U256;
use sp_runtime::{
	traits::{Block as BlockT, UniqueSaturatedInto},
	AccountId32, Percent, Permill,
};
use sp_state_machine::InspectState;
use state_chain_runtime::{
	chainflip::{
		ethereum_elections::{EthereumKeyManagerEvent, VaultEvents},
		BlockUpdate, Offence,
	},
	constants::common::TX_FEE_MULTIPLIER,
	runtime_apis::{
		custom_api::{CustomRuntimeApi, RawWitnessedEvents},
		elections_api::ElectoralRuntimeApi,
		types::{
			AuctionState, BoostPoolDepth, BoostPoolDetails, BrokerInfo, CcmData, ChainAccounts,
			DelegationSnapshot, DispatchErrorWithMessage, EncodedNonNativeCall,
			EncodedNonNativeCallGeneric, EncodingType, EvmCallDetails, FailingWitnessValidators,
			FeeTypes, LendingPosition, LiquidityProviderBoostPoolInfo, LiquidityProviderInfo,
			NetworkFees, NonceOrAccount, OpenedDepositChannels, OperatorInfo,
			RpcAccountInfoCommonItems, RpcLendingConfig, RpcLendingPool, RuntimeApiPenalty,
			SimulateSwapAdditionalOrder, SimulatedSwapInformation, TradingStrategyInfo,
			TradingStrategyLimits, TransactionScreeningEvents, ValidatorInfo, VaultAddresses,
			VaultSwapDetails,
		},
	},
	safe_mode::RuntimeSafeMode,
	Hash, Runtime,
};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	marker::PhantomData,
	sync::Arc,
	time::Duration,
};

pub mod backend;
pub mod broker;
pub mod lp;
pub mod monitoring;
pub mod order_fills;
pub mod pool_client;

#[cfg(test)]
mod tests;

type RpcEncodedNonNativeCall =
	EncodedNonNativeCallGeneric<ethers_core::types::transaction::eip712::TypedData>;

mod chainflip_transparency {
	use super::*;

	#[derive(Serialize, Deserialize, Debug, Clone)]
	pub struct AddressAndExplanation {
		pub name: String,
		pub address: AddressString,
		#[serde(skip_serializing_if = "Option::is_none")]
		pub explanation: Option<String>,
		#[serde(skip_serializing_if = "Option::is_none")]
		pub rotation_policy: Option<String>,
		pub next_predicted_rotation: Option<String>,
	}

	pub type ControlledDepositAddresses = HashMap<ForeignChain, Vec<AddressString>>;

	pub type ControlledVaultAddresses = HashMap<ForeignChain, Vec<AddressAndExplanation>>;
}
use chainflip_transparency::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledSwap {
	pub swap_id: SwapId,
	pub swap_request_id: SwapRequestId,
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub side: Side,
	pub amount: U256,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_asset: Option<Asset>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_amount: Option<U256>,
	pub execute_at: BlockNumber,
	pub remaining_chunks: u32,
	pub chunk_interval: u32,
}

impl ScheduledSwap {
	fn new(
		SwapLegInfo {
			swap_id,
			swap_request_id,
			base_asset,
			quote_asset,
			side,
			amount,
			source_asset,
			source_amount,
			remaining_chunks,
			chunk_interval,
		}: SwapLegInfo,
		execute_at: BlockNumber,
	) -> Self {
		ScheduledSwap {
			swap_id,
			swap_request_id,
			base_asset,
			quote_asset,
			side,
			amount: amount.into(),
			source_asset,
			source_amount: source_amount.map(Into::into),
			execute_at,
			remaining_chunks,
			chunk_interval,
		}
	}
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RpcLiquidityProviderBoostPoolInfo {
	pub fee_tier: u16,
	pub total_balance: U256,
	pub available_balance: U256,
	pub in_use_balance: U256,
	pub is_withdrawing: bool,
}

impl From<&LiquidityProviderBoostPoolInfo> for RpcLiquidityProviderBoostPoolInfo {
	fn from(info: &LiquidityProviderBoostPoolInfo) -> Self {
		// pattern matching to ensure exhaustive use of the fields
		let LiquidityProviderBoostPoolInfo {
			fee_tier,
			total_balance,
			available_balance,
			in_use_balance,
			is_withdrawing,
		} = info;

		Self {
			fee_tier: *fee_tier,
			total_balance: (*total_balance).into(),
			available_balance: (*available_balance).into(),
			in_use_balance: (*in_use_balance).into(),
			is_withdrawing: *is_withdrawing,
		}
	}
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RpcAffiliate {
	pub account_id: AccountId32,
	#[serde(flatten)]
	pub details: AffiliateDetails,
}

impl From<(AccountId32, AffiliateDetails)> for RpcAffiliate {
	fn from((account_id, details): (AccountId32, AffiliateDetails)) -> Self {
		Self { account_id, details }
	}
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RpcAccountInfoWrapper {
	#[serde(flatten)]
	pub common_items: RpcAccountInfoCommonItems<NumberOrHex>,
	#[serde(flatten)]
	pub role_specific: RpcAccountInfo,
}

#[expect(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum RpcAccountInfo {
	Unregistered {},
	Broker {
		#[deprecated(note = "This field is deprecated and will be replaced in a future release")]
		earned_fees: any::AssetMap<NumberOrHex>,
		#[serde(skip_serializing_if = "Vec::is_empty")]
		affiliates: Vec<RpcAffiliate>,
		#[serde(skip_serializing_if = "Option::is_none")]
		btc_vault_deposit_address: Option<String>,
		#[serde(skip_serializing_if = "Option::is_none")]
		bound_fee_withdrawal_address: Option<EthereumAddress>,
	},
	LiquidityProvider {
		refund_addresses: BTreeMap<ForeignChain, Option<ForeignChainAddressHumanreadable>>,
		earned_fees: any::AssetMap<U256>,
		boost_balances: any::AssetMap<Vec<RpcLiquidityProviderBoostPoolInfo>>,
		lending_positions: Vec<LendingPosition<U256>>,
		collateral_balances: Vec<AssetAndAmount<U256>>,
	},
	Validator {
		last_heartbeat: u32,
		reputation_points: i32,
		keyholder_epochs: Vec<u32>,
		is_current_authority: bool,
		#[deprecated]
		is_current_backup: bool,
		is_qualified: bool,
		is_online: bool,
		is_bidding: bool,
		apy_bp: Option<u32>,
		#[serde(skip_serializing_if = "Option::is_none")]
		operator: Option<AccountId32>,
	},
	Operator {
		#[serde(flatten)]
		info: OperatorInfo<NumberOrHex>,
	},
}

impl From<account_info_before_api_v7::RpcAccountInfo> for RpcAccountInfoWrapper {
	fn from(old_value: account_info_before_api_v7::RpcAccountInfo) -> Self {
		use account_info_before_api_v7::RpcAccountInfo as OldRpcAccountInfo;
		match old_value {
			OldRpcAccountInfo::Unregistered { flip_balance, asset_balances } => Self {
				common_items: RpcAccountInfoCommonItems {
					flip_balance,
					asset_balances,
					..Default::default()
				},
				role_specific: RpcAccountInfo::Unregistered {},
			},
			OldRpcAccountInfo::Broker {
				flip_balance,
				bond,
				earned_fees,
				affiliates,
				btc_vault_deposit_address,
			} => Self {
				common_items: RpcAccountInfoCommonItems {
					flip_balance,
					asset_balances: any::AssetMap::default(),
					bond,
					..Default::default()
				},
				role_specific: RpcAccountInfo::Broker {
					earned_fees,
					affiliates,
					btc_vault_deposit_address: btc_vault_deposit_address.map(Into::into),
					bound_fee_withdrawal_address: None,
				},
			},
			OldRpcAccountInfo::LiquidityProvider {
				balances,
				refund_addresses,
				flip_balance,
				earned_fees,
				boost_balances,
			} => Self {
				common_items: RpcAccountInfoCommonItems {
					flip_balance,
					asset_balances: balances,
					bond: Default::default(),
					..Default::default()
				},
				role_specific: RpcAccountInfo::LiquidityProvider {
					refund_addresses: refund_addresses.into_iter().collect(),
					earned_fees,
					boost_balances,
					collateral_balances: vec![],
					lending_positions: vec![],
				},
			},
			OldRpcAccountInfo::Validator {
				flip_balance,
				bond,
				last_heartbeat,
				reputation_points,
				keyholder_epochs,
				is_current_authority,
				is_current_backup,
				is_qualified,
				is_online,
				is_bidding,
				bound_redeem_address,
				apy_bp,
				restricted_balances,
				estimated_redeemable_balance,
			} => Self {
				common_items: RpcAccountInfoCommonItems {
					flip_balance,
					asset_balances: any::AssetMap::default(),
					estimated_redeemable_balance,
					bound_redeem_address,
					restricted_balances,
					bond,
					..Default::default()
				},
				role_specific: RpcAccountInfo::Validator {
					last_heartbeat,
					reputation_points,
					keyholder_epochs,
					is_current_authority,
					is_current_backup,
					is_qualified,
					is_online,
					is_bidding,
					apy_bp,
					operator: None,
				},
			},
		}
	}
}

pub mod account_info_before_api_v7 {
	use super::*;
	use state_chain_runtime::runtime_apis::types::{before_version_10, validator_info_before_v7};

	#[expect(clippy::large_enum_variant)]
	#[derive(Serialize, Deserialize, Clone)]
	#[serde(tag = "role", rename_all = "snake_case")]
	pub enum RpcAccountInfo {
		Unregistered {
			flip_balance: NumberOrHex,
			asset_balances: any::AssetMap<NumberOrHex>,
		},
		Broker {
			flip_balance: NumberOrHex,
			bond: NumberOrHex,
			#[deprecated(
				note = "This field is deprecated and will be replaced in a future release"
			)]
			earned_fees: any::AssetMap<NumberOrHex>,
			#[serde(skip_serializing_if = "Vec::is_empty")]
			affiliates: Vec<RpcAffiliate>,
			#[serde(skip_serializing_if = "Option::is_none")]
			btc_vault_deposit_address: Option<AddressString>,
		},
		LiquidityProvider {
			balances: any::AssetMap<NumberOrHex>,
			refund_addresses: HashMap<ForeignChain, Option<ForeignChainAddressHumanreadable>>,
			flip_balance: NumberOrHex,
			earned_fees: any::AssetMap<U256>,
			boost_balances: any::AssetMap<Vec<RpcLiquidityProviderBoostPoolInfo>>,
		},
		Validator {
			flip_balance: NumberOrHex,
			bond: NumberOrHex,
			last_heartbeat: u32,
			reputation_points: i32,
			keyholder_epochs: Vec<u32>,
			is_current_authority: bool,
			is_current_backup: bool,
			is_qualified: bool,
			is_online: bool,
			is_bidding: bool,
			bound_redeem_address: Option<EthereumAddress>,
			apy_bp: Option<u32>,
			restricted_balances: BTreeMap<EthereumAddress, NumberOrHex>,
			estimated_redeemable_balance: NumberOrHex,
		},
	}

	impl RpcAccountInfo {
		pub fn unregistered(balance: u128, asset_balances: any::AssetMap<u128>) -> Self {
			Self::Unregistered {
				flip_balance: balance.into(),
				asset_balances: asset_balances.map(Into::into),
			}
		}

		pub fn broker(broker_info: before_version_10::BrokerInfo, balance: u128) -> Self {
			Self::Broker {
				flip_balance: balance.into(),
				bond: broker_info.bond.into(),
				btc_vault_deposit_address: broker_info.btc_vault_deposit_address.map(Into::into),
				earned_fees: cf_chains::assets::any::AssetMap::from_iter_or_default(
					broker_info
						.earned_fees
						.iter()
						.map(|(asset, balance)| (*asset, (*balance).into())),
				),
				affiliates: broker_info.affiliates.into_iter().map(Into::into).collect(),
			}
		}

		pub fn lp(info: LiquidityProviderInfo, network: NetworkEnvironment, balance: u128) -> Self {
			Self::LiquidityProvider {
				flip_balance: balance.into(),
				balances: cf_chains::assets::any::AssetMap::from_iter_or_default(
					info.balances.iter().map(|(asset, balance)| (*asset, (*balance).into())),
				),
				refund_addresses: info
					.refund_addresses
					.into_iter()
					.map(|(chain, address)| (chain, address.map(|a| a.to_humanreadable(network))))
					.collect(),
				earned_fees: info
					.earned_fees
					.iter()
					.map(|(asset, balance)| (asset, (*balance).into()))
					.collect(),
				boost_balances: info
					.boost_balances
					.iter()
					.map(|(asset, infos)| (asset, infos.iter().map(|info| info.into()).collect()))
					.collect(),
			}
		}

		pub fn validator(info: validator_info_before_v7::ValidatorInfo) -> Self {
			Self::Validator {
				flip_balance: info.balance.into(),
				bond: info.bond.into(),
				last_heartbeat: info.last_heartbeat,
				reputation_points: info.reputation_points,
				keyholder_epochs: info.keyholder_epochs,
				is_current_authority: info.is_current_authority,
				is_current_backup: info.is_current_backup,
				is_qualified: info.is_qualified,
				is_online: info.is_online,
				is_bidding: info.is_bidding,
				bound_redeem_address: info.bound_redeem_address,
				apy_bp: info.apy_bp,
				restricted_balances: info
					.restricted_balances
					.into_iter()
					.map(|(address, balance)| (address, balance.into()))
					.collect(),
				estimated_redeemable_balance: info.estimated_redeemable_balance.into(),
			}
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcAccountInfoV2 {
	pub balance: NumberOrHex,
	pub bond: NumberOrHex,
	pub last_heartbeat: u32,
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<u32>,
	pub is_current_authority: bool,
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EthereumAddress>,
	pub apy_bp: Option<u32>,
	pub restricted_balances: BTreeMap<EthereumAddress, u128>,
	pub estimated_redeemable_balance: NumberOrHex,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcPenalty {
	reputation_points: i32,
	suspension_duration_blocks: u32,
}

type RpcSuspensions = Vec<(Offence, Vec<(u32, state_chain_runtime::AccountId)>)>;

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcAuctionState {
	epoch_duration: u32,
	current_epoch_started_at: u32,
	redemption_period_as_percentage: u8,
	min_funding: NumberOrHex,
	min_bid: NumberOrHex,
	auction_size_range: (u32, u32),
	min_active_bid: Option<NumberOrHex>,
}

impl From<AuctionState> for RpcAuctionState {
	fn from(auction_state: AuctionState) -> Self {
		Self {
			epoch_duration: auction_state.epoch_duration,
			current_epoch_started_at: auction_state.current_epoch_started_at,
			redemption_period_as_percentage: auction_state.redemption_period_as_percentage,
			min_funding: auction_state.min_funding.into(),
			min_bid: auction_state.min_bid.into(),
			auction_size_range: auction_state.auction_size_range,
			min_active_bid: auction_state.min_active_bid.map(|bond| bond.into()),
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcSwapOutputV1 {
	// Intermediary amount, if there's any
	pub intermediary: Option<NumberOrHex>,
	// Final output of the swap
	pub output: NumberOrHex,
}

impl From<RpcSwapOutputV2> for RpcSwapOutputV1 {
	fn from(swap_output: RpcSwapOutputV2) -> Self {
		Self {
			intermediary: swap_output.intermediary.map(Into::into),
			output: swap_output.output.into(),
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcFee {
	#[serde(flatten)]
	pub asset: Asset,
	pub amount: AmmAmount,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcSwapOutputV2 {
	// Intermediary amount, if there's any
	pub intermediary: Option<U256>,
	// Final output of the swap
	pub output: U256,
	pub network_fee: RpcFee,
	pub ingress_fee: RpcFee,
	pub egress_fee: RpcFee,
	pub broker_commission: RpcFee,
}
fn into_rpc_swap_output(
	simulated_swap_info: SimulatedSwapInformation,
	from_asset: Asset,
	to_asset: Asset,
) -> RpcSwapOutputV2 {
	RpcSwapOutputV2 {
		intermediary: simulated_swap_info.intermediary.map(Into::into),
		output: simulated_swap_info.output.into(),
		network_fee: RpcFee {
			asset: cf_primitives::STABLE_ASSET,
			amount: simulated_swap_info.network_fee.into(),
		},
		ingress_fee: RpcFee { asset: from_asset, amount: simulated_swap_info.ingress_fee.into() },
		egress_fee: RpcFee { asset: to_asset, amount: simulated_swap_info.egress_fee.into() },
		broker_commission: RpcFee {
			asset: cf_primitives::STABLE_ASSET,
			amount: simulated_swap_info.broker_fee.into(),
		},
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SwapRateV2AdditionalOrder {
	LimitOrder { base_asset: Asset, quote_asset: Asset, side: Side, tick: Tick, sell_amount: U256 },
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct RpcPoolInfo {
	#[serde(flatten)]
	pub pool_info: pallet_cf_pools::before_v13::PoolInfo,
	pub quote_asset: Asset,
}

impl From<pallet_cf_pools::before_v13::PoolInfo> for RpcPoolInfo {
	fn from(pool_info: pallet_cf_pools::before_v13::PoolInfo) -> Self {
		Self { pool_info, quote_asset: Asset::Usdc }
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PoolsEnvironment {
	pub fees: any::AssetMap<Option<RpcPoolInfo>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IngressEgressEnvironment {
	pub minimum_deposit_amounts: any::AssetMap<NumberOrHex>,
	pub ingress_fees: any::AssetMap<Option<NumberOrHex>>,
	pub egress_fees: any::AssetMap<Option<NumberOrHex>>,
	pub witness_safety_margins: HashMap<ForeignChain, Option<u64>>,
	pub egress_dust_limits: any::AssetMap<NumberOrHex>,
	pub channel_opening_fees: HashMap<ForeignChain, NumberOrHex>,
	pub ingress_delays: HashMap<ForeignChain, u32>,
	pub boost_delays: HashMap<ForeignChain, u32>,
	pub boost_minimum_add_funds_amounts: any::AssetMap<NumberOrHex>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FundingEnvironment {
	pub redemption_tax: NumberOrHex,
	pub minimum_funding_amount: NumberOrHex,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SwappingEnvironment {
	maximum_swap_amounts: any::AssetMap<Option<NumberOrHex>>,
	#[deprecated(note = "Use network_fees field instead")]
	network_fee_hundredth_pips: Permill,
	swap_retry_delay_blocks: u32,
	max_swap_retry_duration_blocks: u32,
	max_swap_request_duration_blocks: u32,
	minimum_chunk_size: any::AssetMap<NumberOrHex>,
	network_fees: NetworkFees,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcEnvironment {
	ingress_egress: IngressEgressEnvironment,
	swapping: SwappingEnvironment,
	funding: FundingEnvironment,
	pools: PoolsEnvironment,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PoolPriceV2 {
	pub base_asset: Asset,
	pub quote_asset: Asset,
	#[serde(flatten)]
	pub price: pallet_cf_pools::PoolPriceV2,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcPrewitnessedSwap {
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub side: Side,
	pub amounts: Vec<U256>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SwapResponse {
	swaps: Vec<ScheduledSwap>,
}

type TradingStrategyInfoHexAmounts = TradingStrategyInfo<NumberOrHex>;

mod boost_pool_rpc {

	use std::collections::BTreeSet;

	use cf_primitives::PrewitnessedDepositId;
	use sp_runtime::AccountId32;

	use super::*;

	#[derive(Serialize, Deserialize, Clone)]
	struct AccountAndAmount {
		account_id: AccountId32,
		amount: U256,
	}

	#[derive(Serialize, Deserialize, Clone)]
	struct PendingBoost {
		deposit_id: PrewitnessedDepositId,
		owed_amounts: Vec<AccountAndAmount>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	struct PendingWithdrawal {
		account_id: AccountId32,
		pending_deposits: BTreeSet<PrewitnessedDepositId>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct BoostPoolDetailsRpc {
		fee_tier: u16,
		#[serde(flatten)]
		asset: Asset,
		available_amounts: Vec<AccountAndAmount>,
		deposits_pending_finalization: Vec<PendingBoost>,
		pending_withdrawals: Vec<PendingWithdrawal>,
		network_fee_deduction_percent: Percent,
	}

	impl BoostPoolDetailsRpc {
		pub fn new(asset: Asset, fee_tier: u16, details: BoostPoolDetails<AccountId32>) -> Self {
			BoostPoolDetailsRpc {
				asset,
				fee_tier,
				available_amounts: details
					.available_amounts
					.into_iter()
					.map(|(account_id, amount)| AccountAndAmount {
						account_id,
						amount: U256::from(amount),
					})
					.collect(),
				deposits_pending_finalization: details
					.pending_boosts
					.into_iter()
					.map(|(deposit_id, owed_amounts)| PendingBoost {
						deposit_id,
						owed_amounts: owed_amounts
							.into_iter()
							.map(|(account_id, amount)| AccountAndAmount {
								account_id,
								amount: U256::from(amount.total),
							})
							.collect(),
					})
					.collect(),
				pending_withdrawals: details
					.pending_withdrawals
					.into_iter()
					.map(|(account_id, pending_deposits)| PendingWithdrawal {
						account_id,
						pending_deposits,
					})
					.collect(),
				network_fee_deduction_percent: details.network_fee_deduction_percent,
			}
		}
	}

	#[derive(Serialize, Deserialize, Clone)]
	struct PendingFees {
		deposit_id: PrewitnessedDepositId,
		fees: Vec<AccountAndAmount>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct BoostPoolFeesRpc {
		fee_tier: u16,
		#[serde(flatten)]
		asset: Asset,
		pending_fees: Vec<PendingFees>,
	}

	impl BoostPoolFeesRpc {
		pub fn new(asset: Asset, fee_tier: u16, details: BoostPoolDetails<AccountId32>) -> Self {
			BoostPoolFeesRpc {
				fee_tier,
				asset,
				pending_fees: details
					.pending_boosts
					.into_iter()
					.map(|(deposit_id, owed_amounts)| PendingFees {
						deposit_id,
						fees: owed_amounts
							.into_iter()
							.map(|(account_id, amount)| AccountAndAmount {
								account_id,
								amount: U256::from(amount.fee),
							})
							.collect(),
					})
					.collect(),
			}
		}
	}
}

type BoostPoolDepthResponse = Vec<BoostPoolDepth>;
type BoostPoolDetailsResponse = Vec<boost_pool_rpc::BoostPoolDetailsRpc>;
type BoostPoolFeesResponse = Vec<boost_pool_rpc::BoostPoolFeesRpc>;

/// A wrapper type for bitcoin hashes that serializes the hash in reverse.
#[derive(Debug, Clone, Deserialize)]
pub struct BitcoinHash(pub sp_core::H256);

impl Serialize for BitcoinHash {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		sp_core::H256(self.0.to_fixed_bytes().into_iter().rev().collect_array())
			.serialize(serializer)
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcTransactionRef {
	Bitcoin { hash: BitcoinHash },
	Evm { hash: cf_chains::evm::H256 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcTransactionId {
	Bitcoin { hash: BitcoinHash },
	Evm { signature: cf_chains::evm::SchnorrVerificationComponents },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DepositDetails {
	Bitcoin { tx_id: BitcoinHash, vout: u32 },
	Evm { tx_hashes: Vec<cf_chains::evm::H256> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcDepositWitnessInfo {
	pub deposit_chain_block_height: u64,
	pub deposit_address: AddressString,
	pub amount: NumberOrHex,
	pub asset: cf_chains::assets::any::Asset,
	pub deposit_details: Option<DepositDetails>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastWitnessInfo {
	pub broadcast_chain_block_height: u64,
	pub broadcast_id: cf_primitives::BroadcastId,
	pub tx_out_id: RpcTransactionId,
	pub tx_ref: RpcTransactionRef,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcVaultDepositWitnessInfo {
	pub tx_id: String,
	pub deposit_chain_block_height: u64,
	pub input_asset: cf_chains::assets::any::Asset,
	pub output_asset: cf_chains::assets::any::Asset,
	pub amount: NumberOrHex,
	pub destination_address: AddressString,
	pub ccm_deposit_metadata:
		Option<cf_chains::CcmDepositMetadataUnchecked<cf_chains::ForeignChainAddress>>,
	pub deposit_details: Option<DepositDetails>,
	pub broker_fee: Option<cf_primitives::Beneficiary<AccountId32>>,
	pub affiliate_fees: Vec<cf_primitives::Beneficiary<AccountId32>>,
	pub refund_params: Option<ChannelRefundParametersUnchecked<AddressString>>,
	pub dca_params: Option<DcaParameters>,
	pub max_boost_fee: BasisPoints,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcWitnessedEventsResponse {
	pub deposits: Vec<RpcDepositWitnessInfo>,
	pub broadcasts: Vec<BroadcastWitnessInfo>,
	pub vault_deposits: Vec<RpcVaultDepositWitnessInfo>,
}

pub(crate) fn convert_deposit_witness<C: Chain>(
	witness: &DepositWitness<C>,
	height: u64,
	network: NetworkEnvironment,
) -> RpcDepositWitnessInfo
where
	C::DepositDetails: IntoRpcDepositDetails,
{
	RpcDepositWitnessInfo {
		deposit_chain_block_height: height,
		deposit_address: AddressString::from_encoded_address(
			EncodedAddress::from_chain_account::<C>(witness.deposit_address.clone(), network),
		),
		amount: <<C as cf_chains::Chain>::ChainAmount as Into<u128>>::into(witness.amount).into(),
		asset: witness.asset.into(),
		deposit_details: witness.deposit_details.clone().into_rpc_deposit_details(),
	}
}

pub(crate) fn convert_vault_deposit_witness<T, I>(
	witness: &VaultDepositWitness<T, I>,
	height: u64,
	network: NetworkEnvironment,
) -> RpcVaultDepositWitnessInfo
where
	T: pallet_cf_ingress_egress::Config<I, AccountId = state_chain_runtime::AccountId>,
	I: 'static,
	<T::TargetChain as Chain>::DepositDetails: IntoRpcDepositDetails,
	<T::TargetChain as Chain>::ChainAccount: Clone,
	<<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId:
		IntoTransactionInIdForAnyChain<<T::TargetChain as Chain>::ChainCrypto>,
{
	let tx_id = <<T::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId::into_transaction_in_id_for_any_chain(witness.tx_id.clone())
		.to_string();

	let affiliate_fees: Vec<cf_primitives::Beneficiary<AccountId32>> = witness
		.affiliate_fees
		.iter()
		.filter_map(|affiliate| {
			let broker_id = witness.broker_fee.as_ref().map(|b| &b.account);
			resolve_affiliate_to_account(broker_id, affiliate.account)
				.map(|account| cf_primitives::Beneficiary { account, bps: affiliate.bps })
		})
		.collect();

	let refund_params = Some(witness.refund_params.clone().map_address(|address| {
		AddressString::from_encoded_address(EncodedAddress::from_chain_account::<T::TargetChain>(
			address, network,
		))
	}));

	RpcVaultDepositWitnessInfo {
		tx_id,
		deposit_chain_block_height: height,
		input_asset: witness.input_asset.into(),
		output_asset: witness.output_asset,
		amount: <<<T as pallet_cf_ingress_egress::Config<I>>::TargetChain as Chain>::ChainAmount as Into<u128>>::into(witness.deposit_amount).into(),
		destination_address: AddressString::from_encoded_address(witness.destination_address.clone()),
		ccm_deposit_metadata: witness.deposit_metadata.clone(),
		deposit_details: witness.deposit_details.clone().into_rpc_deposit_details(),
		broker_fee: witness.broker_fee.clone(),
		affiliate_fees,
		refund_params,
		dca_params: witness.dca_params.clone(),
		max_boost_fee: witness.boost_fee,
	}
}

fn resolve_affiliate_to_account(
	broker_id: Option<&state_chain_runtime::AccountId>,
	short_id: cf_primitives::AffiliateShortId,
) -> Option<state_chain_runtime::AccountId> {
	broker_id
		.and_then(|broker| pallet_cf_swapping::AffiliateIdMapping::<Runtime>::get(broker, short_id))
}

fn convert_bitcoin_broadcast(
	tx_confirmation: pallet_cf_broadcast::TransactionConfirmation<Runtime, BitcoinInstance>,
	height: u64,
) -> Option<BroadcastWitnessInfo> {
	let (broadcast_id, _) =
		TransactionOutIdToBroadcastId::<Runtime, BitcoinInstance>::get(tx_confirmation.tx_out_id)?;

	Some(BroadcastWitnessInfo {
		broadcast_chain_block_height: height,
		broadcast_id,
		tx_out_id: RpcTransactionId::Bitcoin { hash: BitcoinHash(tx_confirmation.tx_out_id) },
		tx_ref: RpcTransactionRef::Bitcoin { hash: BitcoinHash(tx_confirmation.transaction_ref) },
	})
}

fn convert_evm_broadcast(
	key_manager_event: &EthereumKeyManagerEvent,
	height: u64,
) -> Option<BroadcastWitnessInfo> {
	match key_manager_event {
		EthereumKeyManagerEvent::SignatureAccepted { tx_out_id, transaction_ref, .. } => {
			let (broadcast_id, _) =
				TransactionOutIdToBroadcastId::<Runtime, EthereumInstance>::get(tx_out_id)?;
			Some(BroadcastWitnessInfo {
				broadcast_chain_block_height: height,
				broadcast_id,
				tx_out_id: RpcTransactionId::Evm { signature: *tx_out_id },
				tx_ref: RpcTransactionRef::Evm { hash: *transaction_ref },
			})
		},
		_ => None,
	}
}

fn extract_vault_deposit_from_event<T, I, C>(
	event: &VaultEvents<VaultDepositWitness<T, I>, C>,
) -> Option<VaultDepositWitness<T, I>>
where
	T: pallet_cf_ingress_egress::Config<I>,
	I: 'static,
	C: Chain,
	VaultDepositWitness<T, I>: Clone,
{
	match event {
		VaultEvents::SwapNativeFilter(w) |
		VaultEvents::SwapTokenFilter(w) |
		VaultEvents::XcallNativeFilter(w) |
		VaultEvents::XcallTokenFilter(w) => Some(w.clone()),
		// TransferNativeFailedFilter and TransferTokenFailedFilter don't contain vault deposits
		_ => None,
	}
}

trait IntoRpcDepositDetails {
	fn into_rpc_deposit_details(self) -> Option<DepositDetails>;
}

impl IntoRpcDepositDetails for cf_chains::btc::Utxo {
	fn into_rpc_deposit_details(self) -> Option<DepositDetails> {
		Some(DepositDetails::Bitcoin { tx_id: BitcoinHash(self.id.tx_id), vout: self.id.vout })
	}
}

impl IntoRpcDepositDetails for cf_chains::evm::DepositDetails {
	fn into_rpc_deposit_details(self) -> Option<DepositDetails> {
		self.tx_hashes.map(|tx_hashes| DepositDetails::Evm { tx_hashes })
	}
}

fn convert_raw_witnessed_events(
	raw: RawWitnessedEvents,
	network: NetworkEnvironment,
) -> RpcWitnessedEventsResponse {
	match raw {
		RawWitnessedEvents::Bitcoin { deposits, broadcasts, vault_deposits } => {
			let deposits = deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_deposit_witness::<cf_chains::Bitcoin>(&witness, height, network)
				})
				.collect();

			let vault_deposits = vault_deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_vault_deposit_witness::<Runtime, BitcoinInstance>(
						&witness, height, network,
					)
				})
				.collect();

			let broadcasts = broadcasts
				.into_iter()
				.filter_map(|(height, tx)| convert_bitcoin_broadcast(tx, height))
				.collect();

			RpcWitnessedEventsResponse { deposits, broadcasts, vault_deposits }
		},
		RawWitnessedEvents::Ethereum { deposits, broadcasts, vault_deposits } => {
			let deposits = deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_deposit_witness::<cf_chains::Ethereum>(&witness, height, network)
				})
				.collect();

			let vault_deposits = vault_deposits
				.into_iter()
				.filter_map(|(height, event)| {
					extract_vault_deposit_from_event::<Runtime, EthereumInstance, cf_chains::Ethereum>(
						&event,
					)
					.map(|witness| {
						convert_vault_deposit_witness::<Runtime, EthereumInstance>(
							&witness, height, network,
						)
					})
				})
				.collect();

			let broadcasts = broadcasts
				.into_iter()
				.filter_map(|(height, event)| convert_evm_broadcast(&event, height))
				.collect();

			RpcWitnessedEventsResponse { deposits, broadcasts, vault_deposits }
		},
		RawWitnessedEvents::Arbitrum { deposits, broadcasts, vault_deposits } => {
			let deposits = deposits
				.into_iter()
				.map(|(height, witness)| {
					convert_deposit_witness::<cf_chains::Arbitrum>(&witness, height, network)
				})
				.collect();

			let vault_deposits = vault_deposits
				.into_iter()
				.filter_map(|(height, event)| {
					extract_vault_deposit_from_event::<Runtime, ArbitrumInstance, cf_chains::Arbitrum>(
						&event,
					)
					.map(|witness| {
						convert_vault_deposit_witness::<Runtime, ArbitrumInstance>(
							&witness, height, network,
						)
					})
				})
				.collect();

			let broadcasts = broadcasts
				.into_iter()
				.filter_map(|(height, event)| convert_evm_broadcast(&event, height))
				.collect();

			RpcWitnessedEventsResponse { deposits, broadcasts, vault_deposits }
		},
	}
}

#[rpc(server, client, namespace = "cf")]
/// The custom RPC endpoints for the state chain node.
pub trait CustomApi {
	/// Returns true if the current phase is the auction phase.
	#[method(name = "is_auction_phase")]
	fn cf_is_auction_phase(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<bool>;
	#[method(name = "eth_key_manager_address")]
	fn cf_eth_key_manager_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<String>;
	#[method(name = "eth_state_chain_gateway_address")]
	fn cf_eth_state_chain_gateway_address(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<String>;
	#[method(name = "eth_flip_token_address")]
	fn cf_eth_flip_token_address(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<String>;
	#[method(name = "eth_chain_id")]
	fn cf_eth_chain_id(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u64>;
	/// Returns the eth vault in the form [agg_key, active_from_eth_block]
	#[method(name = "eth_vault")]
	fn cf_eth_vault(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<(String, u32)>;
	#[method(name = "tx_fee_multiplier")]
	fn cf_tx_fee_multiplier(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u64>;
	// Returns the Auction params in the form [min_set_size, max_set_size]
	#[method(name = "auction_parameters")]
	fn cf_auction_parameters(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<(u32, u32)>;
	#[method(name = "min_funding")]
	fn cf_min_funding(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<NumberOrHex>;
	#[method(name = "current_epoch")]
	fn cf_current_epoch(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "epoch_duration")]
	fn cf_epoch_duration(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "current_epoch_started_at")]
	fn cf_current_epoch_started_at(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "authority_emission_per_block")]
	fn cf_authority_emission_per_block(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<NumberOrHex>;
	#[method(name = "backup_emission_per_block")]
	fn cf_backup_emission_per_block(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<NumberOrHex>;
	#[method(name = "flip_supply")]
	fn cf_flip_supply(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<(NumberOrHex, NumberOrHex)>;
	#[method(name = "accounts")]
	fn cf_accounts(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(state_chain_runtime::AccountId, String)>>;
	#[method(name = "account_info")]
	fn cf_account_info(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfoWrapper>;
	#[deprecated(note = "Please use `cf_account_info` instead.")]
	#[method(name = "account_info_v2")]
	fn cf_account_info_v2(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfoV2>;
	#[method(name = "free_balances", aliases = ["cf_asset_balances"])]
	fn cf_free_balances(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<any::AssetMap<U256>>;
	#[method(name = "lp_total_balances", aliases = ["lp_total_balances"])]
	fn cf_lp_total_balances(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<any::AssetMap<U256>>;
	#[method(name = "penalties")]
	fn cf_penalties(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(Offence, RpcPenalty)>>;
	#[method(name = "suspensions")]
	fn cf_suspensions(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcSuspensions>;
	#[method(name = "generate_gov_key_call_hash")]
	fn cf_generate_gov_key_call_hash(
		&self,
		call: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<GovCallHash>;
	#[method(name = "auction_state")]
	fn cf_auction_state(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<RpcAuctionState>;
	#[method(name = "pool_price")]
	fn cf_pool_price(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolPriceV1>>;
	#[method(name = "pool_price_v2")]
	fn cf_pool_price_v2(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPriceV2>;
	#[method(name = "swap_rate")]
	fn cf_pool_swap_rate(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		amount: NumberOrHex,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutputV1>;
	#[method(name = "swap_rate_v2")]
	fn cf_pool_swap_rate_v2(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		amount: U256,
		additional_orders: Option<Vec<SwapRateV2AdditionalOrder>>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutputV2>;
	#[method(name = "swap_rate_v3")]
	fn cf_pool_swap_rate_v3(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		amount: U256,
		broker_commission: BasisPoints,
		dca_parameters: Option<DcaParameters>,
		ccm_data: Option<CcmData>,
		exclude_fees: Option<BTreeSet<FeeTypes>>,
		additional_orders: Option<Vec<SwapRateV2AdditionalOrder>>,
		is_internal: Option<bool>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutputV2>;
	#[method(name = "required_asset_ratio_for_range_order")]
	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<cf_amm::math::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPairsMap<AmmAmount>>;
	#[method(name = "pool_orderbook")]
	fn cf_pool_orderbook(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		orders: u32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrderbook>;
	#[method(name = "pool_info")]
	fn cf_pool_info(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::before_v13::PoolInfo>;
	#[method(name = "pool_depth")]
	fn cf_pool_depth(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<cf_amm::math::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AskBidMap<UnidirectionalPoolDepth>>;
	#[method(name = "pool_liquidity")]
	fn cf_pool_liquidity(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolLiquidity>;
	#[method(name = "pool_orders")]
	fn cf_pool_orders(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		lp: Option<state_chain_runtime::AccountId>,
		filled_orders: Option<bool>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrders<state_chain_runtime::Runtime>>;
	#[method(name = "pool_range_order_liquidity_value")]
	fn cf_pool_range_order_liquidity_value(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPairsMap<AmmAmount>>;
	#[method(name = "funding_environment")]
	fn cf_funding_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<FundingEnvironment>;
	#[method(name = "swapping_environment")]
	fn cf_swapping_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<SwappingEnvironment>;
	#[method(name = "ingress_egress_environment")]
	fn cf_ingress_egress_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<IngressEgressEnvironment>;
	#[method(name = "pools_environment", aliases = ["cf_pool_environment"])]
	fn cf_pools_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolsEnvironment>;
	#[method(name = "available_pools")]
	fn cf_available_pools(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<PoolPairsMap<Asset>>>;
	#[method(name = "environment")]
	fn cf_environment(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEnvironment>;
	#[deprecated(note = "Use direct storage access of `CurrentReleaseVersion` instead.")]
	#[method(name = "current_compatibility_version")]
	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer>;

	#[method(name = "max_swap_amount")]
	fn cf_max_swap_amount(&self, asset: Asset) -> RpcResult<Option<AssetAmount>>;
	#[subscription(name = "subscribe_pool_price", item = BlockUpdate<PoolPriceV1>)]
	async fn cf_subscribe_pool_price(&self, from_asset: Asset, to_asset: Asset);
	#[subscription(name = "subscribe_pool_price_v2", item = BlockUpdate<PoolPriceV2>)]
	async fn cf_subscribe_pool_price_v2(&self, base_asset: Asset, quote_asset: Asset);

	// Subscribe to a stream that on every block produces a list of all scheduled/pending
	// swaps in the base_asset/quote_asset pool, including any "implicit" half-swaps (as a
	// part of a swap involving two pools)
	#[subscription(name = "subscribe_scheduled_swaps", item = BlockUpdate<SwapResponse>)]
	async fn cf_subscribe_scheduled_swaps(&self, base_asset: Asset, quote_asset: Asset);

	#[subscription(name = "subscribe_lp_order_fills", item = BlockUpdate<OrderFills>)]
	async fn cf_subscribe_lp_order_fills(
		&self,
		notification_behaviour: Option<NotificationBehaviour>,
	);

	#[subscription(name = "subscribe_transaction_screening_events", item = BlockUpdate<TransactionScreeningEvents>)]
	async fn cf_subscribe_transaction_screening_events(&self);

	#[method(name = "lp_get_order_fills")]
	fn cf_lp_get_order_fills(&self, at: Option<Hash>) -> RpcResult<BlockUpdate<OrderFills>>;

	#[method(name = "scheduled_swaps")]
	fn cf_scheduled_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<ScheduledSwap>>;

	#[method(name = "supported_assets")]
	fn cf_supported_assets(&self) -> RpcResult<Vec<Asset>>;

	#[method(name = "failed_call_ethereum")]
	fn cf_failed_call_ethereum(
		&self,
		broadcast_id: BroadcastId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<<cf_chains::Ethereum as Chain>::Transaction>>;

	#[method(name = "failed_call_arbitrum")]
	fn cf_failed_call_arbitrum(
		&self,
		broadcast_id: BroadcastId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<<cf_chains::Arbitrum as Chain>::Transaction>>;

	#[method(name = "witness_count")]
	fn cf_witness_count(
		&self,
		hash: state_chain_runtime::Hash,
		epoch_index: Option<EpochIndex>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<FailingWitnessValidators>>;

	#[method(name = "boost_pools_depth")]
	fn cf_boost_pools_depth(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolDepthResponse>;

	#[method(name = "boost_pool_details")]
	fn cf_boost_pool_details(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolDetailsResponse>;

	#[method(name = "boost_pool_pending_fees")]
	fn cf_boost_pool_pending_fees(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolFeesResponse>;

	#[method(name = "safe_mode_statuses")]
	fn cf_safe_mode_statuses(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RuntimeSafeMode>;

	#[method(name = "solana_electoral_data")]
	fn cf_solana_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "solana_filter_votes")]
	fn cf_solana_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "bitcoin_electoral_data")]
	fn cf_bitcoin_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "bitcoin_filter_votes")]
	fn cf_bitcoin_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "ethereum_electoral_data")]
	fn cf_ethereum_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "ethereum_filter_votes")]
	fn cf_ethereum_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "arbitrum_electoral_data")]
	fn cf_arbitrum_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "arbitrum_filter_votes")]
	fn cf_arbitrum_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "generic_electoral_data")]
	fn cf_generic_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "generic_filter_votes")]
	fn cf_generic_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>>;

	#[method(name = "validate_dca_params")]
	fn cf_validate_dca_params(
		&self,
		number_of_chunks: u32,
		chunk_interval: u32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<()>;

	#[method(name = "validate_refund_params")]
	fn cf_validate_refund_params(
		&self,
		input_asset: Asset,
		output_asset: Asset,
		retry_duration: BlockNumber,
		max_oracle_price_slippage: Option<BasisPoints>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<()>;

	#[method(name = "request_swap_parameter_encoding")]
	fn cf_request_swap_parameter_encoding(
		&self,
		broker: state_chain_runtime::AccountId,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<state_chain_runtime::AccountId>>,
		dca_parameters: Option<DcaParameters>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultSwapDetails<AddressString>>;

	#[method(name = "decode_vault_swap_parameter")]
	fn cf_decode_vault_swap_parameter(
		&self,
		broker: state_chain_runtime::AccountId,
		vault_swap: VaultSwapDetails<AddressString>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultSwapInputRpc>;

	#[method(name = "encode_cf_parameters")]
	fn cf_encode_cf_parameters(
		&self,
		broker: state_chain_runtime::AccountId,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		refund_parameters: RefundParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<state_chain_runtime::AccountId>>,
		dca_parameters: Option<DcaParameters>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcBytes>;

	#[method(name = "get_open_deposit_channels")]
	fn cf_get_open_deposit_channels(
		&self,
		broker: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ChainAccounts>;

	#[method(name = "get_transaction_screening_events")]
	fn cf_get_transaction_screening_events(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<TransactionScreeningEvents>;

	#[method(name = "get_affiliates")]
	fn cf_affiliate_details(
		&self,
		broker: state_chain_runtime::AccountId,
		affiliate: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(state_chain_runtime::AccountId, AffiliateDetails)>>;

	#[method(name = "get_vault_addresses")]
	fn cf_vault_addresses(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultAddresses>;

	#[method(name = "all_open_deposit_channels")]
	fn cf_all_open_deposit_channels(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<OpenedDepositChannels>>;

	#[method(name = "get_trading_strategies")]
	fn cf_get_trading_strategies(
		&self,
		lp: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<TradingStrategyInfoHexAmounts>>;

	#[method(name = "get_trading_strategy_limits")]
	fn cf_trading_strategy_limits(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<TradingStrategyLimits>;

	#[method(name = "oracle_prices")]
	fn cf_oracle_prices(
		&self,
		base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<OraclePrice>>;

	#[method(name = "lending_pools")]
	fn cf_lending_pools(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcLendingPool<U256>>>;

	#[method(name = "loan_accounts")]
	fn cf_loan_accounts(
		&self,
		borrower_id: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcLoanAccount<state_chain_runtime::AccountId, U256>>>;

	#[method(name = "lending_pool_supply_balances")]
	fn cf_lending_pool_supply_balances(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<LendingPoolAndSupplyPositions<state_chain_runtime::AccountId, U256>>>;

	#[method(name = "lending_config")]
	fn cf_lending_config(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcLendingConfig>;

	#[method(name = "evm_calldata")]
	fn cf_evm_calldata(
		&self,
		caller: EthereumAddress,
		call: state_chain_runtime::chainflip::ethereum_sc_calls::EthereumSCApi<NumberOrHex>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<EvmCallDetails>;
	#[method(name = "active_delegations")]
	fn cf_active_delegations(
		&self,
		operator: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<DelegationSnapshot<state_chain_runtime::AccountId, NumberOrHex>>>;
	#[method(name = "encode_non_native_call")]
	fn cf_encode_non_native_call(
		&self,
		call: RpcBytes,
		blocks_to_expiry: BlockNumber,
		nonce_or_account: NonceOrAccount,
		encoding: EncodingType,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<(RpcEncodedNonNativeCall, TransactionMetadata)>;
	#[method(name = "controlled_deposit_addresses")]
	fn cf_controlled_deposit_addresses(
		&self,
		chain: Option<cf_primitives::ForeignChain>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ControlledDepositAddresses>;
	#[method(name = "controlled_vault_addresses")]
	fn cf_controlled_vault_addresses(
		&self,
		chain: Option<cf_primitives::ForeignChain>,
		compact_reply: Option<bool>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ControlledVaultAddresses>;
	/// Returns the witnessed events (deposits, vault deposits, broadcasts) for a given chain
	/// from the block witnesser election's unsynchronized state.
	#[method(name = "ingress_egress")]
	fn cf_ingress_egress(
		&self,
		chain: ForeignChain,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcWitnessedEventsResponse>;
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B, BE> {
	pub rpc_backend: CustomRpcBackend<C, B, BE>,
}

impl<C, B, BE> CustomRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + HeaderBackend<B>,
{
	pub fn new(
		client: Arc<C>,
		backend: Arc<BE>,
		executor: Arc<dyn sp_core::traits::SpawnNamed>,
	) -> Self {
		Self { rpc_backend: CustomRpcBackend::new(client, backend, executor) }
	}
}

pub struct StorageQueryApi<'a, C, B>(&'a C, PhantomData<B>);

impl<'a, C, B> StorageQueryApi<'a, C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + CallApiAt<B>,
{
	pub fn new(client: &'a C) -> Self {
		Self(client, Default::default())
	}

	pub fn get_storage_value<
		V: frame_support::storage::StorageValue<T> + 'static,
		T: codec::FullCodec,
	>(
		&self,
		hash: <B as BlockT>::Hash,
	) -> RpcResult<<V as frame_support::storage::StorageValue<T>>::Query> {
		self.with_state_backend(hash, || V::get())
	}

	pub fn collect_from_storage_map<
		M: frame_support::storage::IterableStorageMap<K, V> + 'static,
		K: codec::FullEncode + codec::Decode,
		V: codec::FullCodec,
		I: FromIterator<(K, V)>,
	>(
		&self,
		hash: <B as BlockT>::Hash,
	) -> RpcResult<I> {
		self.with_state_backend(hash, || M::iter().collect::<I>())
	}

	/// Execute a function that requires access to the state backend externalities environment.
	///
	/// Note that anything that requires access to the state backend should be executed within this
	/// closure. Notably, for example, it's not possible to return an iterator from this function.
	///
	/// Example:
	///
	/// ```ignore
	/// // This works because the storage value is resolved before the closure returns.
	/// let vanity_names = StorageQueryApi::new(client).with_state_backend(hash, || {
	///     pallet_cf_account_roles::VanityNames::get()
	/// });
	///
	/// // This doesn't work because the iterator is a lazy value that is resolved after the closure returns.
	/// let roles = StorageQueryApi::new(client).with_state_backend(hash, || {
	///     pallet_cf_account_roles::AccountRoles::iter()
	/// })
	/// .collect::<Vec<_>>();
	///
	/// // Instead, collect the iterator before returning.
	/// let roles = StorageQueryApi::new(client).with_state_backend(hash, || {
	///     pallet_cf_account_roles::AccountRoles::iter().collect::<Vec<_>>()
	/// });
	/// ```
	pub fn with_state_backend<R>(
		&self,
		hash: <B as BlockT>::Hash,
		f: impl Fn() -> R,
	) -> RpcResult<R> {
		Ok(self.0.state_at(hash).map_err(CfApiError::from)?.inspect_state(f))
	}
}

#[derive(thiserror::Error, Debug)]
pub enum CfApiError {
	#[error("Header not found for block {0:?}")]
	HeaderNotFoundError(Hash),
	#[error(transparent)]
	ClientError(#[from] jsonrpsee::core::client::Error),
	#[error("{0:?}")]
	DispatchError(#[from] DispatchErrorWithMessage),
	#[error("{0:?}")]
	RuntimeApiError(#[from] ApiError),
	#[error("{0:?}")]
	SubstrateClientError(#[from] sc_client_api::blockchain::Error),
	#[error("{0:?}")]
	PoolClientError(#[from] pool_client::PoolClientError),
	#[error("{0:?}")]
	DynamicEventsError(#[from] events_decoder::DynamicEventError),
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
	#[error(transparent)]
	OtherError(#[from] anyhow::Error),
}

impl From<CfApiError> for RpcApiError {
	fn from(error: CfApiError) -> Self {
		match error {
			CfApiError::ClientError(client_error) => RpcApiError::ClientError(client_error),
			CfApiError::DispatchError(dispatch_error) => match dispatch_error {
				DispatchErrorWithMessage::Module(message) |
				DispatchErrorWithMessage::RawMessage(message) => match std::str::from_utf8(&message) {
					Ok(message) => RpcApiError::ErrorObject(call_error(
						std::format!("DispatchError: {message}"),
						CfErrorCode::DispatchError,
					)),
					Err(error) => RpcApiError::ErrorObject(internal_error(format!(
						"Unable to decode Module Error Message: {error}"
					))),
				},
				DispatchErrorWithMessage::Other(error) => RpcApiError::ErrorObject(internal_error(
					format!("Unable to decode DispatchError: {error:?}"),
				)),
			},
			CfApiError::RuntimeApiError(error) => match error {
				ApiError::Application(error) => RpcApiError::ErrorObject(call_error(
					format!("Application error: {error}"),
					CfErrorCode::RuntimeApiError,
				)),
				ApiError::UnknownBlock(error) => RpcApiError::ErrorObject(call_error(
					format!("Unknown block: {error}"),
					CfErrorCode::RuntimeApiError,
				)),
				other => RpcApiError::ErrorObject(internal_error(format!(
					"Unexpected ApiError: {other}"
				))),
			},
			CfApiError::ErrorObject(object) => RpcApiError::ErrorObject(object),
			CfApiError::OtherError(error) => RpcApiError::ErrorObject(internal_error(error)),
			CfApiError::HeaderNotFoundError(_) => RpcApiError::ErrorObject(internal_error(error)),
			CfApiError::SubstrateClientError(error) =>
				RpcApiError::ErrorObject(call_error(error, CfErrorCode::SubstrateClientError)),
			CfApiError::PoolClientError(error) =>
				RpcApiError::ErrorObject(call_error(error, CfErrorCode::PoolClientError)),
			CfApiError::DynamicEventsError(error) =>
				RpcApiError::ErrorObject(call_error(error, CfErrorCode::DynamicEventsError)),
		}
	}
}

#[macro_export]
macro_rules! pass_through {
	($( $name:ident ( $( $arg:ident: $argt:ty ),* $(,)? ) -> $result_type:ty $([map: $mapping:expr])? ),+ $(,)?) => {
		$(
			fn $name(&self, $( $arg: $argt, )* at: Option<state_chain_runtime::Hash>,) -> RpcResult<$result_type> {
				self.rpc_backend.with_runtime_api(at, |api, hash| api.$name(hash, $($arg),* ))
					$(.map($mapping))?
			}
		)+
	};
}

#[macro_export]
macro_rules! pass_through_and_flatten {
	($( $name:ident ( $( $arg:ident: $argt:ty ),* $(,)? ) -> $result_type:ty $([map: $mapping:expr])? ),+ $(,)?) => {
		$(
			fn $name(&self, $( $arg: $argt, )* at: Option<state_chain_runtime::Hash>,) -> RpcResult<$result_type> {
				flatten_into_error(
					self.rpc_backend.with_runtime_api(at, |api, hash| api.$name(hash, $($arg),* ))
						$(.map($mapping))?
				)
			}
		)+
	};
}

fn flatten_into_error<R, E1, E2>(res: Result<Result<R, E1>, E2>) -> Result<R, E2>
where
	CfApiError: From<E1>,
	E2: From<CfApiError>,
{
	match res.map(|inner| inner.map_err(CfApiError::from).map_err(Into::into)) {
		Ok(Ok(r)) => Ok(r),
		Ok(Err(e)) => Err(e),
		Err(e) => Err(e),
	}
}

#[async_trait]
impl<C, B, BE> CustomApiServer for CustomRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	B::Header: Unpin,
	BE: Backend<B> + Send + Sync + 'static,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ ExecutorProvider<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ BlockchainEvents<B>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>,
	C::Api: CustomRuntimeApi<B> + ElectoralRuntimeApi<B>,
{
	pass_through! {
		cf_is_auction_phase() -> bool,
		cf_eth_flip_token_address() -> String [map: hex::encode],
		cf_eth_state_chain_gateway_address() -> String [map: hex::encode],
		cf_eth_key_manager_address() -> String [map: hex::encode],
		cf_eth_chain_id() -> u64,
		cf_eth_vault() -> (String, u32) [map: |(public_key, active_from_block)| (hex::encode(public_key), active_from_block)],
		cf_auction_parameters() -> (u32, u32),
		cf_min_funding() -> NumberOrHex [map: Into::into],
		cf_current_epoch() -> u32,
		cf_epoch_duration() -> u32,
		cf_current_epoch_started_at() -> u32,
		cf_authority_emission_per_block() -> NumberOrHex [map: Into::into],
		cf_backup_emission_per_block() -> NumberOrHex [map: Into::into],
		cf_flip_supply() -> (NumberOrHex, NumberOrHex) [map: |(issuance, offchain_supply)| (issuance.into(), offchain_supply.into())],
		cf_accounts() -> Vec<(state_chain_runtime::AccountId, String)> [map: |accounts| {
			accounts
				.into_iter()
				.map(|(account_id, vanity_name_bytes)| {
					// we can use from_utf8_lossy here because we're guaranteed utf8 when we
					// save the vanity name on the chain
					(account_id, String::from_utf8_lossy(&vanity_name_bytes).into_owned())
				})
				.collect()
		}],
		cf_free_balances(account_id: state_chain_runtime::AccountId) -> AssetMap<U256> [map: |asset_map| asset_map.map(Into::into)],
		cf_lp_total_balances(account_id: state_chain_runtime::AccountId) -> any::AssetMap<U256> [map: |asset_map| asset_map.map(Into::into)],
		cf_penalties() -> Vec<(Offence, RpcPenalty)> [map: |penalties| {
			penalties
				.into_iter()
				.map(|(offence, RuntimeApiPenalty {reputation_points,suspension_duration_blocks})| (
					offence,
					RpcPenalty {
						reputation_points,
						suspension_duration_blocks,
					})
				)
				.collect()
		}],
		cf_suspensions() -> RpcSuspensions,
		cf_generate_gov_key_call_hash(call: Vec<u8>) -> GovCallHash,
		cf_safe_mode_statuses() -> RuntimeSafeMode,
		cf_failed_call_ethereum(broadcast_id: BroadcastId) -> Option<<cf_chains::Ethereum as Chain>::Transaction>,
		cf_failed_call_arbitrum(broadcast_id: BroadcastId) -> Option<<cf_chains::Arbitrum as Chain>::Transaction>,
		cf_boost_pools_depth() -> Vec<BoostPoolDepth>,
		cf_pool_price(from_asset: Asset, to_asset: Asset) -> Option<PoolPriceV1>,
		cf_get_open_deposit_channels(account_id: Option<state_chain_runtime::AccountId>) -> ChainAccounts,
		cf_affiliate_details(broker: state_chain_runtime::AccountId, affiliate: Option<state_chain_runtime::AccountId>) -> Vec<(state_chain_runtime::AccountId, AffiliateDetails)>,
		cf_vault_addresses() -> VaultAddresses,
		cf_all_open_deposit_channels() -> Vec<OpenedDepositChannels>,
		cf_trading_strategy_limits() -> TradingStrategyLimits,
		cf_lending_config() -> RpcLendingConfig,
		cf_auction_state() -> RpcAuctionState [map: Into::into],
	}

	pass_through_and_flatten! {
		cf_required_asset_ratio_for_range_order(base_asset: Asset, quote_asset: Asset, tick_range: Range<Tick>) -> PoolPairsMap<AmmAmount>,
		cf_pool_orderbook(base_asset: Asset, quote_asset: Asset, orders: u32) -> PoolOrderbook,
		cf_pool_depth(base_asset: Asset, quote_asset: Asset, tick_range: Range<Tick>) -> AskBidMap<UnidirectionalPoolDepth>,
		cf_pool_liquidity(base_asset: Asset, quote_asset: Asset) -> PoolLiquidity,
		cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> PoolPairsMap<AmmAmount>,
		cf_validate_dca_params(number_of_chunks: u32, chunk_interval: u32) -> (),
		cf_validate_refund_params(
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			max_oracle_price_slippage: Option<BasisPoints>,
		) -> (),
	}

	fn cf_pool_info(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::before_v13::PoolInfo> {
		flatten_into_error(self.rpc_backend.with_versioned_runtime_api(at, |api, hash, version| {
			if version < 13 {
				api.cf_pool_info_before_version_13(hash, base_asset, quote_asset)
			} else {
				api.cf_pool_info(hash, base_asset, quote_asset).map(|info| info.map(Into::into))
			}
		}))
	}

	fn cf_lending_pools(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcLendingPool<U256>>> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, version| {
			if version < 12 {
				#[expect(deprecated)]
				api.cf_lending_pools_before_version_12(hash, asset)
					.map(|lending_pools| lending_pools.into_iter().map(Into::into).collect())
			} else {
				api.cf_lending_pools(hash, asset)
			}
			.map(|lending_pools| {
				lending_pools
					.into_iter()
					.map(|pool| RpcLendingPool::<U256> {
						asset: pool.asset,
						total_amount: pool.total_amount.into(),
						available_amount: pool.available_amount.into(),
						owed_to_network: pool.owed_to_network.into(),
						utilisation_rate: pool.utilisation_rate,
						current_interest_rate: pool.current_interest_rate,
						config: pool.config,
					})
					.collect()
			})
		})
	}

	fn cf_loan_accounts(
		&self,
		borrower_id: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcLoanAccount<state_chain_runtime::AccountId, U256>>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_loan_accounts(hash, borrower_id).map(|accounts| {
				accounts
					.into_iter()
					.map(|acc| RpcLoanAccount::<_, U256> {
						account: acc.account,
						collateral_topup_asset: acc.collateral_topup_asset,
						ltv_ratio: acc.ltv_ratio,
						collateral: acc.collateral.into_iter().map(Into::into).collect(),
						loans: acc
							.loans
							.into_iter()
							.map(|loan| RpcLoan {
								loan_id: loan.loan_id,
								asset: loan.asset,
								created_at: loan.created_at,
								principal_amount: loan.principal_amount.into(),
							})
							.collect(),
						liquidation_status: acc.liquidation_status,
					})
					.collect()
			})
		})
	}

	fn cf_lending_pool_supply_balances(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<LendingPoolAndSupplyPositions<state_chain_runtime::AccountId, U256>>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_lending_pool_supply_balances(hash, asset).map(|pools_and_positions| {
				pools_and_positions
					.into_iter()
					.map(|pool_and_positions| LendingPoolAndSupplyPositions {
						asset: pool_and_positions.asset,
						positions: pool_and_positions
							.positions
							.into_iter()
							.map(|position| LendingSupplyPosition {
								lp_id: position.lp_id,
								total_amount: position.total_amount.into(),
							})
							.collect(),
					})
					.collect()
			})
		})
	}

	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer> {
		#[expect(deprecated)]
		self.rpc_backend
			.with_runtime_api(None, |api, hash| api.cf_current_compatibility_version(hash))
	}

	fn cf_max_swap_amount(&self, asset: Asset) -> RpcResult<Option<AssetAmount>> {
		self.rpc_backend
			.with_runtime_api(None, |api, hash| api.cf_max_swap_amount(hash, asset))
	}

	fn cf_tx_fee_multiplier(&self, _at: Option<Hash>) -> RpcResult<u64> {
		Ok(TX_FEE_MULTIPLIER as u64)
	}

	fn cf_witness_count(
		&self,
		call_hash: Hash,
		epoch_index: Option<EpochIndex>,
		at: Option<Hash>,
	) -> RpcResult<Option<FailingWitnessValidators>> {
		self.rpc_backend.with_runtime_api(at, |api, block_hash| {
			api.cf_witness_count(
				block_hash,
				pallet_cf_witnesser::CallHash(call_hash.into()),
				epoch_index,
			)
		})
	}

	fn cf_pool_orders(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		lp: Option<state_chain_runtime::AccountId>,
		filled_orders: Option<bool>,
		at: Option<Hash>,
	) -> RpcResult<PoolOrders<state_chain_runtime::Runtime>> {
		flatten_into_error(self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_pool_orders(hash, base_asset, quote_asset, lp, filled_orders.unwrap_or_default())
		}))
	}
	fn cf_pool_price_v2(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPriceV2> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(PoolPriceV2 {
				base_asset,
				quote_asset,
				price: api.cf_pool_price_v2(hash, base_asset, quote_asset)??,
			})
		})
	}

	fn cf_account_info(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfoWrapper> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, api_version| {
			if api_version < 7 {
				use account_info_before_api_v7::RpcAccountInfo as RpcAccountInfoLegacy;
				let balance = api.cf_account_flip_balance(hash, &account_id)?;
				let asset_balances = api.cf_free_balances(hash, account_id.clone())?;

				Ok::<_, CfApiError>(RpcAccountInfoWrapper::from(
					match api
						.cf_account_role(hash, account_id.clone())?
						.unwrap_or(AccountRole::Unregistered)
					{
						AccountRole::Unregistered =>
							RpcAccountInfoLegacy::unregistered(balance, asset_balances),
						AccountRole::Broker => {
							let info = if api_version < 3 {
								#[expect(deprecated)]
								api.cf_broker_info_before_version_3(hash, account_id.clone())?
									.into()
							} else {
								#[expect(deprecated)]
								api.cf_broker_info_before_version_10(hash, account_id.clone())?
							};

							RpcAccountInfoLegacy::broker(info, balance)
						},
						AccountRole::LiquidityProvider => RpcAccountInfoLegacy::lp(
							#[expect(deprecated)]
							api.cf_liquidity_provider_info_before_version_9(
								hash,
								account_id.clone(),
							)?
							.into(),
							api.cf_network_environment(hash)?,
							balance,
						),
						AccountRole::Validator => {
							#[expect(deprecated)]
							let info = api.cf_validator_info_before_version_7(hash, &account_id)?;

							RpcAccountInfoLegacy::validator(info)
						},
						// No other roles existed before v7
						_ =>
							return Err(CfApiError::ErrorObject(ErrorObject::owned(
								ErrorCode::InvalidParams.code(),
								"Unknown Account Role.",
								None::<()>,
							))),
					},
				))
			} else {
				let role = api
					.cf_account_role(hash, account_id.clone())?
					.unwrap_or(AccountRole::Unregistered);
				let common_items = api
					.cf_common_account_info(hash, &account_id)?
					.try_map_balances(TryInto::try_into)
					.map_err(|_| {
						CfApiError::ErrorObject(ErrorObject::owned(
							ErrorCode::InternalError.code(),
							"Unable to convert balances.",
							None::<()>,
						))
					})?;

				Ok(RpcAccountInfoWrapper {
					common_items,
					role_specific: match role {
						AccountRole::Unregistered => RpcAccountInfo::Unregistered {},
						AccountRole::Broker => {
							let BrokerInfo {
								earned_fees,
								btc_vault_deposit_address,
								affiliates,
								bound_fee_withdrawal_address,
								..
							} = if api_version < 10 {
								#[expect(deprecated)]
								api.cf_broker_info_before_version_10(hash, account_id.clone())?
									.into()
							} else {
								let network = api.cf_network_environment(hash)?.into();
								if api_version < 15 {
									#[expect(deprecated)]
									api.cf_broker_info_before_version_15(hash, account_id.clone())?
										.into()
								} else {
									api.cf_broker_info(hash, account_id.clone())?
								}
								.map(|pubkey| pubkey.to_address(&network))
							};
							RpcAccountInfo::Broker {
								earned_fees: AssetMap::from_iter_or_default(
									earned_fees.into_iter().map(|(k, v)| (k, v.into())),
								),
								affiliates: affiliates.into_iter().map(Into::into).collect(),
								btc_vault_deposit_address,
								bound_fee_withdrawal_address,
							}
						},
						AccountRole::LiquidityProvider => {
							#[expect(deprecated)]
							let LiquidityProviderInfo {
								refund_addresses,
								earned_fees,
								boost_balances,
								lending_positions,
								collateral_balances,
								..
							} = if api_version < 9 {
								api.cf_liquidity_provider_info_before_version_9(
									hash,
									account_id.clone(),
								)?
								.into()
							} else {
								api.cf_liquidity_provider_info(hash, account_id)?
							};

							let network = api.cf_network_environment(hash)?;
							RpcAccountInfo::LiquidityProvider {
								refund_addresses: refund_addresses
									.into_iter()
									.map(|(chain, address)| {
										(chain, address.map(|a| a.to_humanreadable(network)))
									})
									.collect(),
								earned_fees: earned_fees
									.iter()
									.map(|(asset, balance)| (asset, (*balance).into()))
									.collect(),
								boost_balances: boost_balances
									.iter()
									.map(|(asset, infos)| {
										(asset, infos.iter().map(|info| info.into()).collect())
									})
									.collect(),
								lending_positions: lending_positions
									.into_iter()
									.map(|pos| LendingPosition {
										asset: pos.asset,
										total_amount: pos.total_amount.into(),
										available_amount: pos.available_amount.into(),
									})
									.collect(),
								collateral_balances: collateral_balances
									.into_iter()
									.map(|(asset, amount)| AssetAndAmount {
										asset,
										amount: amount.into(),
									})
									.collect(),
							}
						},
						AccountRole::Validator => {
							let ValidatorInfo {
								last_heartbeat,
								reputation_points,
								keyholder_epochs,
								is_current_authority,
								is_current_backup,
								is_qualified,
								is_online,
								is_bidding,
								apy_bp,
								operator,
								..
							} = api.cf_validator_info(hash, &account_id)?;
							RpcAccountInfo::Validator {
								last_heartbeat,
								reputation_points,
								keyholder_epochs,
								is_current_authority,
								is_current_backup,
								is_qualified,
								is_online,
								is_bidding,
								apy_bp,
								operator,
							}
						},
						AccountRole::Operator => RpcAccountInfo::Operator {
							info: api
								.cf_operator_info(hash, &account_id)?
								.map_amounts(NumberOrHex::from),
						},
					},
				})
			}
		})
	}

	fn cf_account_info_v2(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<RpcAccountInfoV2> {
		let account_info = self
			.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_validator_info(hash, &account_id))?;

		Ok(RpcAccountInfoV2 {
			balance: account_info.balance.into(),
			bond: account_info.bond.into(),
			last_heartbeat: account_info.last_heartbeat,
			reputation_points: account_info.reputation_points,
			keyholder_epochs: account_info.keyholder_epochs,
			is_current_authority: account_info.is_current_authority,
			is_current_backup: account_info.is_current_backup,
			is_qualified: account_info.is_qualified,
			is_online: account_info.is_online,
			is_bidding: account_info.is_bidding,
			bound_redeem_address: account_info.bound_redeem_address,
			apy_bp: account_info.apy_bp,
			restricted_balances: account_info.restricted_balances,
			estimated_redeemable_balance: account_info.estimated_redeemable_balance.into(),
		})
	}

	fn cf_pool_swap_rate(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		amount: NumberOrHex,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutputV1> {
		self.cf_pool_swap_rate_v2(from_asset, to_asset, amount.into(), None, at)
			.map(Into::into)
	}

	fn cf_pool_swap_rate_v2(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		amount: U256,
		additional_orders: Option<Vec<SwapRateV2AdditionalOrder>>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutputV2> {
		self.cf_pool_swap_rate_v3(
			from_asset,
			to_asset,
			amount,
			Default::default(),
			None,
			None,
			None,
			additional_orders,
			None,
			at,
		)
	}

	fn cf_pool_swap_rate_v3(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		amount: U256,
		broker_commission: BasisPoints,
		dca_parameters: Option<DcaParameters>,
		ccm_data: Option<CcmData>,
		exclude_fees: Option<BTreeSet<FeeTypes>>,
		additional_orders: Option<Vec<SwapRateV2AdditionalOrder>>,
		is_internal: Option<bool>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutputV2> {
		let amount = amount
			.try_into()
			.map_err(|_| "Swap input amount too large.")
			.and_then(|amount: u128| {
				if amount == 0 {
					Err("Swap input amount cannot be zero.")
				} else {
					Ok(amount)
				}
			})
			.map_err(|s| ErrorObject::owned(ErrorCode::InvalidParams.code(), s, None::<()>))?;

		if let Some(CcmData { message_length, .. }) = ccm_data {
			if message_length > MAX_CCM_MSG_LENGTH {
				return Err(RpcApiError::ErrorObject(ErrorObject::owned(
					ErrorCode::InvalidParams.code(),
					"CCM message size too large.",
					None::<()>,
				)));
			}
		}

		let additional_orders = additional_orders.map(|additional_orders| {
			additional_orders
				.into_iter()
				.map(|additional_order| match additional_order {
					SwapRateV2AdditionalOrder::LimitOrder {
						base_asset,
						quote_asset,
						side,
						tick,
						sell_amount,
					} => SimulateSwapAdditionalOrder::LimitOrder {
						base_asset,
						quote_asset,
						side,
						tick,
						sell_amount: sell_amount.unique_saturated_into(),
					},
				})
				.collect()
		});
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(
				api.cf_pool_simulate_swap(
					hash,
					from_asset,
					to_asset,
					amount,
					broker_commission,
					dca_parameters,
					ccm_data,
					exclude_fees.unwrap_or_default(),
					additional_orders,
					is_internal,
				)?
				.map(|simulated_swap_info_v2| {
					into_rpc_swap_output(simulated_swap_info_v2, from_asset, to_asset)
				})?,
			)
		})
	}

	fn cf_ingress_egress_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<IngressEgressEnvironment> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, version| {
			let mut witness_safety_margins = HashMap::new();
			let mut channel_opening_fees = HashMap::new();
			let mut ingress_delays = HashMap::new();
			let mut boost_delays = HashMap::new();

			for chain in ForeignChain::iter() {
				witness_safety_margins.insert(chain, api.cf_witness_safety_margin(hash, chain)?);
				channel_opening_fees.insert(chain, api.cf_channel_opening_fee(hash, chain)?.into());
				// These fields were added in version 8 of the runtime API
				if version >= 8 {
					ingress_delays.insert(chain, api.cf_ingress_delay(hash, chain)?);
					boost_delays.insert(chain, api.cf_boost_delay(hash, chain)?);
				}
			}

			Ok::<_, CfApiError>(IngressEgressEnvironment {
				minimum_deposit_amounts: any::AssetMap::try_from_fn(|asset| {
					api.cf_min_deposit_amount(hash, asset).map(Into::into)
				})?,
				ingress_fees: any::AssetMap::try_from_fn(|asset| {
					api.cf_ingress_fee(hash, asset).map(|value| value.map(Into::into))
				})?,
				egress_fees: any::AssetMap::try_from_fn(|asset| {
					api.cf_egress_fee(hash, asset).map(|value| value.map(Into::into))
				})?,
				witness_safety_margins,
				egress_dust_limits: any::AssetMap::try_from_fn(|asset| {
					api.cf_egress_dust_limit(hash, asset).map(Into::into)
				})?,
				channel_opening_fees,
				ingress_delays,
				boost_delays,
				boost_minimum_add_funds_amounts: if version >= 14 {
					any::AssetMap::try_from_fn(|asset| {
						api.cf_boost_config(hash).map(|config| {
							config
								.minimum_add_funds_amount
								.get(&asset)
								.cloned()
								.unwrap_or(1_u128)
								.into()
						})
					})?
				} else {
					any::AssetMap::default()
				},
			})
		})
	}

	fn cf_swapping_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<SwappingEnvironment> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			let swap_limits = api.cf_swap_limits(hash)?;
			Ok::<_, CfApiError>(SwappingEnvironment {
				maximum_swap_amounts: any::AssetMap::try_from_fn(|asset| {
					api.cf_max_swap_amount(hash, asset).map(|option| option.map(Into::into))
				})?,
				network_fee_hundredth_pips: api
					.cf_network_fees(hash)?
					.regular_network_fee
					.standard_rate_and_minimum
					.rate,
				swap_retry_delay_blocks: api.cf_swap_retry_delay_blocks(hash)?,
				max_swap_retry_duration_blocks: swap_limits.max_swap_retry_duration_blocks,
				max_swap_request_duration_blocks: swap_limits.max_swap_request_duration_blocks,
				minimum_chunk_size: any::AssetMap::try_from_fn(|asset| {
					api.cf_minimum_chunk_size(hash, asset).map(Into::into)
				})?,
				network_fees: api.cf_network_fees(hash)?,
			})
		})
	}

	fn cf_funding_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<FundingEnvironment> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(FundingEnvironment {
				redemption_tax: api.cf_redemption_tax(hash)?.into(),
				minimum_funding_amount: api.cf_min_funding(hash)?.into(),
			})
		})
	}

	fn cf_pools_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolsEnvironment> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(PoolsEnvironment {
				fees: {
					let mut map = AssetMap::default();
					for asset_pair in api.cf_pools(hash).map_err(CfApiError::from)? {
						map[asset_pair.base] = self
							.cf_pool_info(asset_pair.base, asset_pair.quote, at)
							.ok()
							.map(Into::into);
					}
					map
				},
			})
		})
	}

	fn cf_environment(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEnvironment> {
		Ok(RpcEnvironment {
			ingress_egress: self.cf_ingress_egress_environment(at)?,
			swapping: self.cf_swapping_environment(at)?,
			funding: self.cf_funding_environment(at)?,
			pools: self.cf_pools_environment(at)?,
		})
	}

	async fn cf_subscribe_pool_price(
		&self,
		pending_sink: PendingSubscriptionSink,
		from_asset: Asset,
		to_asset: Asset,
	) {
		self.rpc_backend
			.new_subscription(
				Default::default(), /* notification_behaviour */
				true,               /* only_on_changes */
				false,              /* end_on_error */
				pending_sink,
				move |client, hash| {
					Ok((*client.runtime_api())
						.cf_pool_price(hash, from_asset, to_asset)
						.map_err(CfApiError::from)?)
				},
			)
			.await
	}

	async fn cf_subscribe_pool_price_v2(
		&self,
		pending_sink: PendingSubscriptionSink,
		base_asset: Asset,
		quote_asset: Asset,
	) {
		self.rpc_backend
			.new_subscription(
				Default::default(), /* notification_behaviour */
				false,              /* only_on_changes */
				true,               /* end_on_error */
				pending_sink,
				move |client, hash| {
					Ok(PoolPriceV2 {
						base_asset,
						quote_asset,
						price: (*client.runtime_api())
							.cf_pool_price_v2(hash, base_asset, quote_asset)
							.map_err(CfApiError::from)?
							.map_err(CfApiError::from)?,
					})
				},
			)
			.await
	}

	async fn cf_subscribe_transaction_screening_events(
		&self,
		pending_sink: PendingSubscriptionSink,
	) {
		self.rpc_backend
			.new_subscription(
				NotificationBehaviour::Finalized, /* only_finalized */
				false,                            /* only_on_changes */
				true,                             /* end_on_error */
				pending_sink,
				move |client, hash| {
					Ok((*client.runtime_api())
						.cf_transaction_screening_events(hash)
						.map_err(CfApiError::from)?)
				},
			)
			.await;
	}

	async fn cf_subscribe_scheduled_swaps(
		&self,
		pending_sink: PendingSubscriptionSink,
		base_asset: Asset,
		quote_asset: Asset,
	) {
		// Check that the requested pool exists:
		let Ok(Ok(_)) = self.rpc_backend.client.runtime_api().cf_pool_info(
			self.rpc_backend.client.info().best_hash,
			base_asset,
			quote_asset,
		) else {
			pending_sink
				.reject(call_error("requested pool does not exist", CfErrorCode::OtherError))
				.await;
			return;
		};

		self.rpc_backend
			.new_subscription(
				Default::default(), /* notification_behaviour */
				false,              /* only_on_changes */
				true,               /* end_on_error */
				pending_sink,
				move |client, hash| {
					Ok(SwapResponse {
						swaps: (*client.runtime_api())
							.cf_scheduled_swaps(hash, base_asset, quote_asset)
							.map_err(CfApiError::from)?
							.into_iter()
							.map(|(swap, execute_at)| ScheduledSwap::new(swap, execute_at))
							.collect(),
					})
				},
			)
			.await;
	}

	fn cf_scheduled_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<ScheduledSwap>> {
		// Check that the requested pool exists:
		let _ = (*self.rpc_backend.client.runtime_api())
			.cf_pool_info(self.rpc_backend.client.info().best_hash, base_asset, quote_asset)
			.map_err(CfApiError::from)?;

		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_scheduled_swaps(hash, base_asset, quote_asset))
			.map(|swaps| {
				swaps
					.into_iter()
					.map(|(swap, execute_at)| ScheduledSwap::new(swap, execute_at))
					.collect()
			})
	}

	async fn cf_subscribe_lp_order_fills(
		&self,
		sink: PendingSubscriptionSink,
		notification_behaviour: Option<NotificationBehaviour>,
	) {
		self.rpc_backend
			.new_subscription(
				notification_behaviour.unwrap_or(NotificationBehaviour::Finalized),
				false,
				true,
				sink,
				move |client, hash| order_fills::order_fills_for_block(client, hash),
			)
			.await
	}

	fn cf_lp_get_order_fills(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BlockUpdate<OrderFills>> {
		order_fills::order_fills_for_block(
			self.rpc_backend.client.as_ref(),
			at.unwrap_or_else(|| self.rpc_backend.client.info().finalized_hash),
		)
	}

	fn cf_supported_assets(&self) -> RpcResult<Vec<Asset>> {
		Ok(Asset::all().collect())
	}

	fn cf_boost_pool_details(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolDetailsResponse> {
		execute_for_all_or_one_asset(asset, |asset| {
			self.rpc_backend.with_runtime_api(at, |api, hash| {
				api.cf_boost_pool_details(hash, asset).map(|details_for_each_pool| {
					details_for_each_pool
						.into_iter()
						.map(|(tier, details)| BoostPoolDetailsRpc::new(asset, tier, details))
						.collect()
				})
			})
		})
	}

	fn cf_boost_pool_pending_fees(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolFeesResponse> {
		execute_for_all_or_one_asset(asset, |asset| {
			self.rpc_backend.with_runtime_api(at, |api, hash| {
				api.cf_boost_pool_details(hash, asset).map(|details_for_each_pool| {
					details_for_each_pool
						.into_iter()
						.map(|(fee_tier, details)| BoostPoolFeesRpc::new(asset, fee_tier, details))
						.collect()
				})
			})
		})
	}

	fn cf_available_pools(&self, at: Option<Hash>) -> RpcResult<Vec<PoolPairsMap<Asset>>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| api.cf_pools(hash))
	}

	fn cf_solana_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_solana_electoral_data(hash, validator))
	}

	fn cf_solana_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_solana_filter_votes(hash, validator, proposed_votes)
		})
	}

	fn cf_bitcoin_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_bitcoin_electoral_data(hash, validator))
	}

	fn cf_bitcoin_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_bitcoin_filter_votes(hash, validator, proposed_votes)
		})
	}

	fn cf_ethereum_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_ethereum_electoral_data(hash, validator))
	}

	fn cf_ethereum_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_ethereum_filter_votes(hash, validator, proposed_votes)
		})
	}

	fn cf_generic_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_generic_electoral_data(hash, validator))
	}

	fn cf_generic_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_generic_filter_votes(hash, validator, proposed_votes)
		})
	}

	fn cf_arbitrum_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_arbitrum_electoral_data(hash, validator))
	}

	fn cf_arbitrum_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			api.cf_arbitrum_filter_votes(hash, validator, proposed_votes)
		})
	}

	fn cf_request_swap_parameter_encoding(
		&self,
		broker: state_chain_runtime::AccountId,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<state_chain_runtime::AccountId>>,
		dca_parameters: Option<DcaParameters>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultSwapDetails<AddressString>> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, api_version| {
			Ok::<_, CfApiError>(if api_version < 10 {
				#[expect(deprecated)]
				api.cf_request_swap_parameter_encoding_before_version_10(
					hash,
					broker,
					source_asset,
					destination_asset,
					destination_address.try_parse_to_encoded_address(destination_asset.into())?,
					broker_commission,
					try_into_swap_extra_params_encoded(extra_parameters, source_asset.into())?,
					channel_metadata,
					boost_fee.unwrap_or_default(),
					affiliate_fees.unwrap_or_default(),
					dca_parameters,
				)??
				.map_btc_address(Into::into)
			} else {
				let network = api.cf_network_environment(hash)?.into();
				api.cf_request_swap_parameter_encoding(
					hash,
					broker,
					source_asset,
					destination_asset,
					destination_address.try_parse_to_encoded_address(destination_asset.into())?,
					broker_commission,
					try_into_swap_extra_params_encoded(extra_parameters, source_asset.into())?,
					channel_metadata,
					boost_fee.unwrap_or_default(),
					affiliate_fees.unwrap_or_default(),
					dca_parameters,
				)??
				.map_btc_address(|pubkey| pubkey.to_address(&network).into())
			})
		})
	}

	fn cf_decode_vault_swap_parameter(
		&self,
		broker: AccountId32,
		vault_swap: VaultSwapDetails<AddressString>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultSwapInputRpc> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(vault_swap_input_encoded_to_rpc(
				api.cf_decode_vault_swap_parameter(
					hash,
					broker,
					vault_swap.map_btc_address(Into::into),
				)??,
			))
		})
	}

	fn cf_encode_cf_parameters(
		&self,
		broker: state_chain_runtime::AccountId,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		refund_parameters: RefundParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<state_chain_runtime::AccountId>>,
		dca_parameters: Option<DcaParameters>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcBytes> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(
				api.cf_encode_cf_parameters(
					hash,
					broker,
					source_asset,
					destination_address.try_parse_to_encoded_address(destination_asset.into())?,
					destination_asset,
					refund_parameters.parse_refund_address_for_chain(source_asset.into())?,
					dca_parameters,
					boost_fee.unwrap_or_default(),
					broker_commission,
					affiliate_fees.unwrap_or_default(),
					channel_metadata,
				)??
				.into(),
			)
		})
	}

	fn cf_get_transaction_screening_events(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<TransactionScreeningEvents> {
		self.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_transaction_screening_events(hash))
	}

	fn cf_get_trading_strategies(
		&self,
		lp: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<TradingStrategyInfo<NumberOrHex>>> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			let api_version = api
				.api_version::<dyn CustomRuntimeApi<state_chain_runtime::Block>>(hash)
				.map_err(CfApiError::from)?
				.unwrap_or_default();

			let strategies = if api_version < 4 {
				// Strategies didn't exist in earlier versions:
				vec![]
			} else {
				api.cf_get_trading_strategies(hash, lp)
					.map_err(CfApiError::from)?
					.into_iter()
					.map(|info| TradingStrategyInfo {
						lp_id: info.lp_id,
						strategy_id: info.strategy_id,
						strategy: info.strategy,
						balance: info
							.balance
							.into_iter()
							.map(|(asset, amount)| (asset, amount.into()))
							.collect(),
					})
					.collect()
			};

			Ok::<_, CfApiError>(strategies)
		})
	}

	fn cf_evm_calldata(
		&self,
		caller: EthereumAddress,
		call: state_chain_runtime::chainflip::ethereum_sc_calls::EthereumSCApi<NumberOrHex>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<EvmCallDetails> {
		self.rpc_backend.with_runtime_api(at, |api, hash| {
			let api_version = api
				.api_version::<dyn CustomRuntimeApi<state_chain_runtime::Block>>(hash)
				.map_err(CfApiError::from)?
				.unwrap_or_default();

			if api_version < 6 {
				// sc calls via ethereum didn't exist before version 6
				Err(CfApiError::ErrorObject(call_error(
					"sc calls via ethereum are not supported for the current runtime api version",
					CfErrorCode::RuntimeApiError,
				)))
			} else {
				api.cf_evm_calldata(
					hash,
					caller,
					call.try_fmap(TryInto::try_into).map_err(|s| {
						CfApiError::ErrorObject(ErrorObject::owned(
							ErrorCode::InvalidParams.code(),
							format!("Failed to convert call parameters: {s}."),
							None::<()>,
						))
					})?,
				)?
				.map_err(CfApiError::from)
			}
		})
	}

	fn cf_active_delegations(
		&self,
		operator: Option<AccountId32>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<DelegationSnapshot<AccountId32, NumberOrHex>>> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, version| {
			if version < 7 {
				Err(CfApiError::ErrorObject(call_error(
					"Delegations are not supported at this runtime api version",
					CfErrorCode::RuntimeApiError,
				)))
			} else {
				api.cf_active_delegations(hash, operator)
					.map_err(CfApiError::from)?
					.into_iter()
					.map(|delegation| delegation.try_map_bids(TryInto::try_into))
					.try_collect()
					.map_err(|s| {
						CfApiError::ErrorObject(ErrorObject::owned(
							ErrorCode::InvalidParams.code(),
							format!("Failed to convert call parameters: {s}."),
							None::<()>,
						))
					})
			}
		})
	}

	fn cf_encode_non_native_call(
		&self,
		call: RpcBytes,
		blocks_to_expiry: BlockNumber,
		nonce_or_account: NonceOrAccount,
		encoding: EncodingType,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<(RpcEncodedNonNativeCall, TransactionMetadata)> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, version| {
			if version < 8 {
				Err(CfApiError::ErrorObject(call_error(
					"Encoding of non native calls are not supported at this runtime api version",
					CfErrorCode::RuntimeApiError,
				)))
			} else {
				let (encoded_call, metadata) = api
					.cf_encode_non_native_call(
						hash,
						call.into(),
						blocks_to_expiry,
						nonce_or_account,
						encoding,
					)
					.map_err(CfApiError::from)?
					.map_err(|e| {
						CfApiError::ErrorObject(ErrorObject::owned(
							ErrorCode::InternalError.code(),
							format!("Failed to encode non native call: {}", e),
							None::<()>,
						))
					})?;
				let serialized_call = match encoded_call {
					EncodedNonNativeCall::Eip712(typed_data) => RpcEncodedNonNativeCall::Eip712(
						to_ethers_typed_data(typed_data).map_err(|e| {
							CfApiError::ErrorObject(ErrorObject::owned(
								ErrorCode::InternalError.code(),
								e,
								None::<()>,
							))
						})?,
					),
					EncodedNonNativeCall::String(s) => RpcEncodedNonNativeCall::String(s),
				};
				// Return the `transaction_metadata` because it will need
				// to be submitted as part of the `non_native_signed_call`
				// and it has been modified in the runtime api.
				Ok((serialized_call, metadata))
			}
		})
	}

	fn cf_controlled_deposit_addresses(
		&self,
		requested_chain: Option<cf_primitives::ForeignChain>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ControlledDepositAddresses> {
		Ok(self
			.cf_all_open_deposit_channels(at)?
			.into_iter()
			.flat_map(|(_, _, accounts)| {
				accounts
					.chain_accounts
					.into_iter()
					.filter(|(address, _asset)| {
						requested_chain.is_none_or(|chain| chain == address.chain())
					})
					.map(|(address, _asset)| {
						(address.chain(), AddressString::from_encoded_address(address))
					})
			})
			.into_group_map())
	}

	fn cf_controlled_vault_addresses(
		&self,
		chain: Option<cf_primitives::ForeignChain>,
		compact_reply: Option<bool>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ControlledVaultAddresses> {
		let mut result = HashMap::new();

		let VaultAddresses {
			ethereum,
			arbitrum,
			bitcoin,
			sol_vault_program: _,
			sol_swap_endpoint_program_data_account: _,
			usdc_token_mint_pubkey: _,
			bitcoin_vault,
			solana_sol_vault,
			solana_usdc_token_vault_ata,
			solana_vault_swap_account,
			predicted_seconds_until_next_vault_rotation,
		} = self.cf_vault_addresses(at)?;

		let compact_reply = compact_reply.is_some_and(|compact| compact);

		let none_if_compact = |value| if compact_reply { None } else { Some(value) };

		let rotates_every_3_days = none_if_compact("Every 3 days".to_string());
		let rotates_never = none_if_compact("Never".to_string());
		let next_predicted_rotation = (chrono::Utc::now() +
			Duration::from_secs(predicted_seconds_until_next_vault_rotation))
		.to_string();

		if chain.is_none_or(|chain| chain == ForeignChain::Arbitrum) {
			result.insert(ForeignChain::Arbitrum, vec![AddressAndExplanation {
				name: "arbitrum_vault_contract".into(),
				address: AddressString::from_encoded_address(arbitrum),
				explanation: none_if_compact("Holds ETH and all tokens on Arbitrum. Directly receives user funds in case of smart contract-based vault swaps.".into()),
				rotation_policy: rotates_never.clone(),
				next_predicted_rotation: None,
			}]);
		}

		if chain.is_none_or(|chain| chain == ForeignChain::Ethereum) {
			result.insert(ForeignChain::Ethereum, vec![AddressAndExplanation {
				name: "ethereum_vault_contract".into(),
				address: AddressString::from_encoded_address(ethereum),
				explanation: none_if_compact("Holds ETH and all tokens on Ethereum. Directly receives user funds for smart contract-based vault swaps.".into()),
				rotation_policy: rotates_never.clone(),
				next_predicted_rotation: None,
			}]);
		}

		if chain.is_none_or(|chain| chain == ForeignChain::Solana) {
			let mut solana_addresses = Vec::new();
			if let Some(solana_sol_vault) = solana_sol_vault {
				solana_addresses.push(AddressAndExplanation {
					name: "solana_sol_vault".into(),
					address: AddressString::from_encoded_address(solana_sol_vault),
					explanation: none_if_compact("Holds SOL on Solana.".into()),
					rotation_policy: rotates_every_3_days.clone(),
					next_predicted_rotation: Some(next_predicted_rotation.clone()),
				})
			}
			solana_addresses.push(AddressAndExplanation {
				name: "solana_usdc_vault".into(),
				address: AddressString::from_encoded_address(solana_usdc_token_vault_ata),
				explanation: none_if_compact(
					"Holds USDC on Solana. Directly receives user funds for USDC vault swaps."
						.into(),
				),
				rotation_policy: rotates_never.clone(),
				next_predicted_rotation: None,
			});
			if let Some(solana_vault_swap_account) = solana_vault_swap_account {
				solana_addresses.push(AddressAndExplanation {
					name: "solana_sol_vault_swap_account".into(),
					address: AddressString::from_encoded_address(solana_vault_swap_account),
					explanation: none_if_compact("Special account for vault swap support for SOL on Solana. Receives user funds for SOL vault swaps before they are fetched into the vault.".into()),
					rotation_policy: rotates_never,
					next_predicted_rotation: None,
				})
			}
			result.insert(ForeignChain::Solana, solana_addresses);
		}

		if chain.is_none_or(|chain| chain == ForeignChain::Bitcoin) {
			let mut bitcoin_addresses = Vec::new();
			if let Some(bitcoin_vault) = bitcoin_vault {
				bitcoin_addresses.push(AddressAndExplanation {
					name: "bitcoin_vault".into(),
					address: AddressString::from_encoded_address(bitcoin_vault),
					explanation: none_if_compact("Holds BTC on Bitcoin.".into()),
					rotation_policy: rotates_every_3_days.clone(),
					next_predicted_rotation: Some(next_predicted_rotation.clone()),
				});
			}
			bitcoin_addresses.extend(bitcoin.into_iter().map(|(broker_id, address)| {
				AddressAndExplanation {
					name: format!("bitcoin_vault_swap_address_for_broker_{broker_id}"),
					address: AddressString::from_encoded_address(address),
					explanation: none_if_compact(
						"Special per-broker address for vault swap support on Bitcoin. Receives user funds for BTC vault swaps before they are fetched into the vault."
							.into()),
					rotation_policy: rotates_every_3_days.clone(),
					next_predicted_rotation: Some(next_predicted_rotation.clone()),
				}
			}));
			result.insert(ForeignChain::Bitcoin, bitcoin_addresses);
		}

		Ok(result)
	}

	fn cf_oracle_prices(
		&self,
		base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<OraclePrice>> {
		self.rpc_backend.with_versioned_runtime_api(at, |api, hash, version| {
			Ok::<_, CfApiError>(if version < 11 {
				#[expect(deprecated)]
				api.cf_oracle_prices_before_version_11(hash, base_and_quote_asset)
					.map_err(CfApiError::from)?
					.into_iter()
					.map(Into::into)
					.collect()
			} else {
				api.cf_oracle_prices(hash, base_and_quote_asset).map_err(CfApiError::from)?
			})
		})
	}
	fn cf_ingress_egress(
		&self,
		chain: ForeignChain,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcWitnessedEventsResponse> {
		let hash = self.rpc_backend.unwrap_or_best(at);
		let raw = self
			.rpc_backend
			.with_runtime_api(Some(hash), |api, hash| api.cf_ingress_egress(hash, chain))?
			.map_err(CfApiError::from)?;
		let network = self
			.rpc_backend
			.with_runtime_api(Some(hash), |api, hash| api.cf_network_environment(hash))?;

		Ok(convert_raw_witnessed_events(raw.clone(), network))
	}
}

/// Execute f (which returns a Vec of results) for `asset`. If `asset` is `None`
/// the closure is executed for every supported asset and the results are concatenated.
fn execute_for_all_or_one_asset<Response, F>(
	asset: Option<Asset>,
	mut f: F,
) -> RpcResult<Vec<Response>>
where
	F: FnMut(Asset) -> RpcResult<Vec<Response>>,
{
	if let Some(asset) = asset {
		f(asset)
	} else {
		let results_for_each_asset: RpcResult<Vec<_>> = Asset::all().map(f).collect();

		results_for_each_asset.map(|inner| inner.into_iter().flatten().collect())
	}
}

/// Returns the preallocated channel IDs for a given account and chain on the last finalized block.
fn get_preallocated_channels<C, B, BE>(
	rpc_backend: &CustomRpcBackend<C, B, BE>,
	account_id: AccountId32,
	chain: ForeignChain,
) -> RpcResult<Vec<ChannelId>>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Backend<B> + Send + Sync + 'static,
	C: sp_api::ProvideRuntimeApi<B> + HeaderBackend<B> + Send + Sync + 'static,
	C::Api: CustomRuntimeApi<B>,
{
	rpc_backend.with_runtime_api(Some(rpc_backend.client.info().finalized_hash), |api, hash| {
		api.cf_get_preallocated_deposit_channels(hash, account_id, chain)
	})
}
