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

pub mod types;

use crate::runtime_apis::types::*;

use crate::{chainflip::Offence, Runtime, RuntimeSafeMode};
use cf_amm::{
	common::PoolPairsMap,
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	self, address::EncodedAddress, assets::any::AssetMap, eth::Address as EthereumAddress,
	CcmChannelMetadataUnchecked, Chain, ChannelRefundParametersUncheckedEncoded,
	VaultSwapExtraParametersEncoded, VaultSwapInputEncoded,
};
use cf_primitives::{
	AccountRole, Affiliates, Asset, AssetAmount, BasisPoints, BlockNumber, BroadcastId, ChannelId,
	DcaParameters, EpochIndex, FlipBalance, ForeignChain, NetworkEnvironment, SemVer,
};
use cf_traits::SwapLimits;
use core::{ops::Range, str};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_elections::electoral_systems::oracle_price::{
	chainlink::OraclePrice, price::PriceAsset,
};
pub use pallet_cf_environment::TransactionMetadata;
use pallet_cf_governance::GovCallHash;
pub use pallet_cf_ingress_egress::ChannelAction;
pub use pallet_cf_lending_pools::BoostPoolDetails;
use pallet_cf_pools::{
	AskBidMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_swapping::{AffiliateDetails, SwapLegInfo};
use pallet_cf_witnesser::CallHash;
use scale_info::prelude::string::String;

use sp_api::decl_runtime_apis;

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
//
// Versioning of runtime apis is explained here:
// https://docs.rs/sp-api/latest/sp_api/macro.decl_runtime_apis.html
// Of course it doesn't explain everything, e.g. there's a very useful
// `#[renamed($OLD_NAME, $VERSION)]` attribute which will handle renaming
// of apis automatically.
decl_runtime_apis!(
	#[api_version(9)]
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
		#[deprecated(note = "The notion of backup nodes is no longer used.")]
		fn cf_backup_emission_per_block() -> u128;
		/// Returns the flip supply in the form [total_issuance, offchain_funds]
		fn cf_flip_supply() -> (u128, u128);
		fn cf_accounts() -> Vec<(AccountId32, VanityName)>;
		fn cf_account_flip_balance(account_id: &AccountId32) -> u128;
		#[changed_in(7)]
		fn cf_validator_info(account_id: &AccountId32) -> validator_info_before_v7::ValidatorInfo;
		fn cf_validator_info(account_id: &AccountId32) -> ValidatorInfo;
		#[changed_in(7)]
		fn cf_operator_info();
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
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			max_oracle_price_slippage: Option<BasisPoints>,
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
		fn cf_lending_pools(asset: Option<Asset>) -> Vec<RpcLendingPool<AssetAmount>>;
		fn cf_loan_accounts(
			borrower_id: Option<AccountId32>,
		) -> Vec<RpcLoanAccount<AccountId32, AssetAmount>>;
		fn cf_lending_pool_supply_balances(
			asset: Option<Asset>,
		) -> Vec<LendingPoolAndSupplyPositions<AccountId32, AssetAmount>>;
		fn cf_lending_config() -> RpcLendingConfig;
		fn cf_evm_calldata(
			caller: EthereumAddress,
			call: crate::chainflip::ethereum_sc_calls::EthereumSCApi<FlipBalance>,
		) -> Result<EvmCallDetails, DispatchErrorWithMessage>;
		#[changed_in(6)]
		fn cf_evm_calldata();
		#[changed_in(7)]
		fn cf_common_account_info();
		fn cf_common_account_info(
			account_id: &AccountId32,
		) -> RpcAccountInfoCommonItems<FlipBalance>;
		#[changed_in(7)]
		fn cf_active_delegations();
		fn cf_active_delegations(
			account: Option<AccountId32>,
		) -> Vec<DelegationSnapshot<AccountId32, FlipBalance>>;
		#[changed_in(8)]
		fn cf_ingress_delay();
		fn cf_ingress_delay(chain: ForeignChain) -> u32;
		#[changed_in(8)]
		fn cf_boost_delay();
		fn cf_boost_delay(chain: ForeignChain) -> u32;
		#[changed_in(9)]
		fn cf_encode_non_native_call();
		fn cf_encode_non_native_call(
			call: Vec<u8>,
			blocks_to_expiry: BlockNumber,
			nonce_or_account: NonceOrAccount,
			encoding: EncodingType,
		) -> Result<(EncodedNonNativeCall, TransactionMetadata), DispatchErrorWithMessage>;
	}
);
