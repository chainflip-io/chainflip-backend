use crate::chainflip::Offence;
use cf_amm::{
	common::{Amount, Order, Tick},
	range_orders::Liquidity,
};
use cf_chains::{eth::Address as EthereumAddress, Chain, ForeignChainAddress};
use cf_primitives::{
	AccountRole, Asset, AssetAmount, BroadcastId, EpochIndex, ForeignChain, NetworkEnvironment,
	SemVer, SwapOutput,
};
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::sp_runtime::AccountId32;
use pallet_cf_governance::GovCallHash;
use pallet_cf_pools::{
	AskBidMap, AssetsMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1,
	PoolPriceV2, UnidirectionalPoolDepth,
};
use pallet_cf_witnesser::CallHash;
use scale_info::{prelude::string::String, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_runtime::DispatchError;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

type VanityName = Vec<u8>;

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
pub struct RuntimeApiAccountInfoV2 {
	pub balance: u128,
	pub bond: u128,
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
	pub restricted_balances: BTreeMap<EthereumAddress, u128>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RuntimeApiPenalty {
	pub reputation_points: i32,
	pub suspension_duration_blocks: u32,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct AuctionState {
	pub blocks_per_epoch: u32,
	pub current_epoch_started_at: u32,
	pub redemption_period_as_percentage: u8,
	pub min_funding: u128,
	pub auction_size_range: (u32, u32),
	pub min_active_bid: Option<u128>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum DispatchErrorWithMessage {
	Module(Vec<u8>),
	Other(DispatchError),
}
impl From<DispatchError> for DispatchErrorWithMessage {
	fn from(value: DispatchError) -> Self {
		match value {
			DispatchError::Module(sp_runtime::ModuleError { message: Some(message), .. }) =>
				DispatchErrorWithMessage::Module(message.as_bytes().to_vec()),
			value => DispatchErrorWithMessage::Other(value),
		}
	}
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct FailingWitnessValidators {
	pub failing_count: u32,
	pub validators: Vec<(cf_primitives::AccountId, String, bool)>,
}

decl_runtime_apis!(
	/// Definition for all runtime API interfaces.
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
		fn cf_account_info_v2(account_id: &AccountId32) -> RuntimeApiAccountInfoV2;
		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)>;
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId32)>)>;
		fn cf_generate_gov_key_call_hash(call: Vec<u8>) -> GovCallHash;
		fn cf_auction_state() -> AuctionState;
		fn cf_pool_price(from: Asset, to: Asset) -> Option<PoolPriceV1>;
		fn cf_pool_price_v2(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolPriceV2, DispatchErrorWithMessage>;
		fn cf_pool_simulate_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
		) -> Result<SwapOutput, DispatchErrorWithMessage>;
		fn cf_pool_info(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolInfo, DispatchErrorWithMessage>;
		fn cf_pool_depth(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::common::Tick>,
		) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage>;
		fn cf_pool_liquidity(
			base_asset: Asset,
			quote_asset: Asset,
		) -> Result<PoolLiquidity, DispatchErrorWithMessage>;
		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::common::Tick>,
		) -> Result<AssetsMap<Amount>, DispatchErrorWithMessage>;
		fn cf_pool_orderbook(
			base_asset: Asset,
			quote_asset: Asset,
			orders: u32,
		) -> Result<PoolOrderbook, DispatchErrorWithMessage>;
		fn cf_pool_orders(
			base_asset: Asset,
			quote_asset: Asset,
			lp: Option<AccountId32>,
		) -> Result<PoolOrders<crate::Runtime>, DispatchErrorWithMessage>;
		fn cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> Result<AssetsMap<Amount>, DispatchErrorWithMessage>;

		fn cf_max_swap_amount(asset: Asset) -> Option<AssetAmount>;
		fn cf_min_deposit_amount(asset: Asset) -> AssetAmount;
		fn cf_egress_dust_limit(asset: Asset) -> AssetAmount;
		fn cf_prewitness_swaps(
			base_asset: Asset,
			quote_asset: Asset,
			side: Order,
		) -> Vec<AssetAmount>;
		fn cf_liquidity_provider_info(account_id: AccountId32) -> Option<LiquidityProviderInfo>;
		fn cf_account_role(account_id: AccountId32) -> Option<AccountRole>;
		fn cf_asset_balances(account_id: AccountId32) -> Vec<(Asset, AssetAmount)>;
		fn cf_redemption_tax() -> AssetAmount;
		fn cf_network_environment() -> NetworkEnvironment;
		fn cf_failed_call(
			broadcast_id: BroadcastId,
		) -> Option<<cf_chains::Ethereum as Chain>::Transaction>;
		fn cf_ingress_fee(asset: Asset) -> Option<AssetAmount>;
		fn cf_egress_fee(asset: Asset) -> Option<AssetAmount>;
		fn cf_witness_count(hash: CallHash) -> Option<FailingWitnessValidators>;
		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64>;
	}
);
