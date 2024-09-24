use crate::boost_pool_rpc::BoostPoolFeesRpc;
use boost_pool_rpc::BoostPoolDetailsRpc;
use cf_amm::{
	common::{Amount, PoolPairsMap, Side, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{ForeignChainAddressHumanreadable, ToHumanreadableAddress},
	dot::PolkadotAccountId,
	eth::Address as EthereumAddress,
	sol::SolAddress,
	Chain,
};
use cf_primitives::{
	chains::assets::any::{self, AssetMap},
	AccountRole, Asset, AssetAmount, BlockNumber, BroadcastId, EpochIndex, ForeignChain,
	NetworkEnvironment, SemVer, SwapId, SwapRequestId,
};
use cf_utilities::rpc::NumberOrHex;
use core::ops::Range;
use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned},
	PendingSubscriptionSink,
};
use order_fills::OrderFills;
use pallet_cf_governance::GovCallHash;
use pallet_cf_pools::{AskBidMap, PoolInfo, PoolLiquidity, PoolPriceV1, UnidirectionalPoolDepth};
use pallet_cf_swapping::SwapLegInfo;
use sc_client_api::{BlockchainEvents, HeaderBackend};
use serde::{Deserialize, Serialize};
use sp_api::{ApiError, CallApiAt};
use sp_core::U256;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto},
	Permill,
};
use sp_state_machine::InspectState;
use state_chain_runtime::{
	chainflip::{BlockUpdate, Offence},
	constants::common::TX_FEE_MULTIPLIER,
	monitoring_apis::{
		ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, EpochState, ExternalChainsBlockHeight,
		FeeImbalance, FlipSupply, LastRuntimeUpgradeInfo, MonitoringData, OpenDepositChannels,
		PendingBroadcasts, PendingTssCeremonies, RedemptionsInfo, SolanaNonces,
	},
	runtime_apis::{
		BoostPoolDepth, BoostPoolDetails, BrokerInfo, CustomRuntimeApi, DispatchErrorWithMessage,
		ElectoralRuntimeApi, FailingWitnessValidators, LiquidityProviderBoostPoolInfo,
		LiquidityProviderInfo, ValidatorInfo,
	},
	safe_mode::RuntimeSafeMode,
	Hash, NetworkFee, SolanaInstance,
};
use std::{
	collections::{BTreeMap, HashMap},
	marker::PhantomData,
	sync::Arc,
};

pub mod monitoring;
pub mod order_fills;

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcEpochState {
	pub blocks_per_epoch: u32,
	pub current_epoch_started_at: u32,
	pub current_epoch_index: u32,
	pub min_active_bid: Option<NumberOrHex>,
	pub rotation_phase: String,
}
impl From<EpochState> for RpcEpochState {
	fn from(rotation_state: EpochState) -> Self {
		Self {
			blocks_per_epoch: rotation_state.blocks_per_epoch,
			current_epoch_started_at: rotation_state.current_epoch_started_at,
			current_epoch_index: rotation_state.current_epoch_index,
			rotation_phase: rotation_state.rotation_phase,
			min_active_bid: rotation_state.min_active_bid.map(Into::into),
		}
	}
}
#[derive(Serialize, Deserialize, Clone)]
pub struct RpcRedemptionsInfo {
	pub total_balance: NumberOrHex,
	pub count: u32,
}
impl From<RedemptionsInfo> for RpcRedemptionsInfo {
	fn from(redemption_info: RedemptionsInfo) -> Self {
		Self { total_balance: redemption_info.total_balance.into(), count: redemption_info.count }
	}
}

pub type RpcFeeImbalance = FeeImbalance<NumberOrHex>;

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcFlipSupply {
	pub total_supply: NumberOrHex,
	pub offchain_supply: NumberOrHex,
}
impl From<FlipSupply> for RpcFlipSupply {
	fn from(flip_supply: FlipSupply) -> Self {
		Self {
			total_supply: flip_supply.total_supply.into(),
			offchain_supply: flip_supply.offchain_supply.into(),
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcMonitoringData {
	pub external_chains_height: ExternalChainsBlockHeight,
	pub btc_utxos: BtcUtxos,
	pub epoch: RpcEpochState,
	pub pending_redemptions: RpcRedemptionsInfo,
	pub pending_broadcasts: PendingBroadcasts,
	pub pending_tss: PendingTssCeremonies,
	pub open_deposit_channels: OpenDepositChannels,
	pub fee_imbalance: RpcFeeImbalance,
	pub authorities: AuthoritiesInfo,
	pub build_version: LastRuntimeUpgradeInfo,
	pub suspended_validators: Vec<(Offence, u32)>,
	pub pending_swaps: u32,
	pub dot_aggkey: PolkadotAccountId,
	pub flip_supply: RpcFlipSupply,
	pub sol_aggkey: SolAddress,
	pub sol_onchain_key: SolAddress,
	pub sol_nonces: SolanaNonces,
	pub activating_key_broadcast_ids: ActivateKeysBroadcastIds,
}
impl From<MonitoringData> for RpcMonitoringData {
	fn from(monitoring_data: MonitoringData) -> Self {
		Self {
			epoch: monitoring_data.epoch.into(),
			pending_redemptions: monitoring_data.pending_redemptions.into(),
			fee_imbalance: monitoring_data.fee_imbalance.map(|i| (*i).into()),
			external_chains_height: monitoring_data.external_chains_height,
			btc_utxos: monitoring_data.btc_utxos,
			pending_broadcasts: monitoring_data.pending_broadcasts,
			pending_tss: monitoring_data.pending_tss,
			open_deposit_channels: monitoring_data.open_deposit_channels,
			authorities: monitoring_data.authorities,
			build_version: monitoring_data.build_version,
			suspended_validators: monitoring_data.suspended_validators,
			pending_swaps: monitoring_data.pending_swaps,
			dot_aggkey: monitoring_data.dot_aggkey,
			flip_supply: monitoring_data.flip_supply.into(),
			sol_aggkey: monitoring_data.sol_aggkey,
			sol_onchain_key: monitoring_data.sol_onchain_key,
			sol_nonces: monitoring_data.sol_nonces,
			activating_key_broadcast_ids: monitoring_data.activating_key_broadcast_ids,
		}
	}
}

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

#[derive(Serialize, Deserialize, Clone)]
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

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum RpcAccountInfo {
	Unregistered {
		flip_balance: NumberOrHex,
	},
	Broker {
		flip_balance: NumberOrHex,
		earned_fees: any::AssetMap<NumberOrHex>,
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
	},
}

impl RpcAccountInfo {
	fn unregistered(balance: u128) -> Self {
		Self::Unregistered { flip_balance: balance.into() }
	}

	fn broker(balance: u128, broker_info: BrokerInfo) -> Self {
		Self::Broker {
			flip_balance: balance.into(),
			earned_fees: cf_chains::assets::any::AssetMap::from_iter_or_default(
				broker_info
					.earned_fees
					.iter()
					.map(|(asset, balance)| (*asset, (*balance).into())),
			),
		}
	}

	fn lp(info: LiquidityProviderInfo, network: NetworkEnvironment, balance: u128) -> Self {
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

	fn validator(info: ValidatorInfo) -> Self {
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
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcPenalty {
	reputation_points: i32,
	suspension_duration_blocks: u32,
}

type RpcSuspensions = Vec<(Offence, Vec<(u32, state_chain_runtime::AccountId)>)>;

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcAuctionState {
	blocks_per_epoch: u32,
	current_epoch_started_at: u32,
	redemption_period_as_percentage: u8,
	min_funding: NumberOrHex,
	auction_size_range: (u32, u32),
	min_active_bid: Option<NumberOrHex>,
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
	pub amount: Amount,
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
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SwapRateV2AdditionalOrder {
	LimitOrder { base_asset: Asset, quote_asset: Asset, side: Side, tick: Tick, sell_amount: U256 },
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct RpcPoolInfo {
	#[serde(flatten)]
	pub pool_info: PoolInfo,
	pub quote_asset: Asset,
}

impl From<PoolInfo> for RpcPoolInfo {
	fn from(pool_info: PoolInfo) -> Self {
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
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FundingEnvironment {
	pub redemption_tax: NumberOrHex,
	pub minimum_funding_amount: NumberOrHex,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SwappingEnvironment {
	maximum_swap_amounts: any::AssetMap<Option<NumberOrHex>>,
	network_fee_hundredth_pips: Permill,
	swap_retry_delay_blocks: u32,
	max_swap_retry_duration_blocks: u32,
	max_swap_request_duration_blocks: u32,
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
	}

	impl BoostPoolDetailsRpc {
		pub fn new(asset: Asset, fee_tier: u16, details: BoostPoolDetails) -> Self {
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
		pub fn new(asset: Asset, fee_tier: u16, details: BoostPoolDetails) -> Self {
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
	) -> RpcResult<RpcAccountInfo>;
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
	#[method(name = "required_asset_ratio_for_range_order")]
	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPairsMap<Amount>>;
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
	) -> RpcResult<PoolInfo>;
	#[method(name = "pool_depth")]
	fn cf_pool_depth(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<cf_amm::common::Tick>,
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
	) -> RpcResult<PoolPairsMap<Amount>>;
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
	#[subscription(name = "subscribe_pool_price", item = PoolPriceV1)]
	fn cf_subscribe_pool_price(&self, from_asset: Asset, to_asset: Asset);
	#[subscription(name = "subscribe_pool_price_v2", item = BlockUpdate<PoolPriceV2>)]
	fn cf_subscribe_pool_price_v2(&self, base_asset: Asset, quote_asset: Asset);
	#[subscription(name = "subscribe_prewitness_swaps", item = BlockUpdate<RpcPrewitnessedSwap>)]
	fn cf_subscribe_prewitness_swaps(&self, base_asset: Asset, quote_asset: Asset, side: Side);

	// Subscribe to a stream that on every block produces a list of all scheduled/pending
	// swaps in the base_asset/quote_asset pool, including any "implicit" half-swaps (as a
	// part of a swap involving two pools)
	#[subscription(name = "subscribe_scheduled_swaps", item = BlockUpdate<SwapResponse>)]
	fn cf_subscribe_scheduled_swaps(&self, base_asset: Asset, quote_asset: Asset);

	#[subscription(name = "subscribe_lp_order_fills", item = BlockUpdate<OrderFills>)]
	fn cf_subscribe_lp_order_fills(&self);

	#[method(name = "scheduled_swaps")]
	fn cf_scheduled_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<ScheduledSwap>>;

	#[method(name = "prewitness_swaps")]
	fn cf_prewitness_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcPrewitnessedSwap>;

	#[method(name = "supported_assets")]
	fn cf_supported_assets(&self) -> RpcResult<Vec<Asset>>;

	#[method(name = "failed_call_ethereum")]
	fn cf_failed_call_ethereum(
		&self,
		broadcast_id: BroadcastId,
	) -> RpcResult<Option<<cf_chains::Ethereum as Chain>::Transaction>>;

	#[method(name = "failed_call_arbitrum")]
	fn cf_failed_call_arbitrum(
		&self,
		broadcast_id: BroadcastId,
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
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B> {
	pub client: Arc<C>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
	pub _phantom: PhantomData<B>,
}

impl<C, B> CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + HeaderBackend<B>,
{
	fn unwrap_or_best(&self, from_rpc: Option<<B as BlockT>::Hash>) -> B::Hash {
		from_rpc.unwrap_or_else(|| self.client.info().best_hash)
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
		Ok(self.0.state_at(hash).map_err(to_rpc_error)?.inspect_state(f))
	}
}

pub fn str_to_rpc_error(e: &str) -> ErrorObjectOwned {
	ErrorObject::owned(jsonrpsee::types::error::CALL_EXECUTION_FAILED_CODE, e, Option::<()>::None)
}

pub fn to_rpc_error<E: std::fmt::Debug + Send + Sync + 'static>(e: E) -> ErrorObjectOwned {
	str_to_rpc_error(&format!("{:?}", e)[..])
}

fn map_dispatch_error(e: DispatchErrorWithMessage) -> ErrorObjectOwned {
	str_to_rpc_error( &(match e {
		DispatchErrorWithMessage::Module(message) => match std::str::from_utf8(&message) {
			Ok(message) => format!("DispatchError: {message}"),
			Err(error) =>
				format!("DispatchError: Unable to deserialize error message: '{error}'"),
		},
		DispatchErrorWithMessage::Other(e) =>
			format!("DispatchError: {}", <&'static str>::from(e)),
	})[..])
}

impl<C, B> CustomApiServer for CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ HeaderBackend<B>
		+ BlockchainEvents<B>
		+ CallApiAt<B>,
	C::Api: CustomRuntimeApi<B> + ElectoralRuntimeApi<B, SolanaInstance>,
{
	fn cf_is_auction_phase(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<bool> {
		self.client
			.runtime_api()
			.cf_is_auction_phase(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}
	fn cf_eth_flip_token_address(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<String> {
		self.client
			.runtime_api()
			.cf_eth_flip_token_address(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(hex::encode)
	}
	fn cf_eth_state_chain_gateway_address(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<String> {
		self.client
			.runtime_api()
			.cf_eth_state_chain_gateway_address(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(hex::encode)
	}
	fn cf_eth_key_manager_address(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<String> {
		self.client
			.runtime_api()
			.cf_eth_key_manager_address(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(hex::encode)
	}
	fn cf_eth_chain_id(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u64> {
		self.client
			.runtime_api()
			.cf_eth_chain_id(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}
	fn cf_eth_vault(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<(String, u32)> {
		self.client
			.runtime_api()
			.cf_eth_vault(self.unwrap_or_best(at))
			.map(|(public_key, active_from_block)| (hex::encode(public_key), active_from_block))
			.map_err(to_rpc_error)
	}
	// FIXME: Respect the block hash argument here
	fn cf_tx_fee_multiplier(&self, _at: Option<<B as BlockT>::Hash>) -> RpcResult<u64> {
		Ok(TX_FEE_MULTIPLIER as u64)
	}
	fn cf_auction_parameters(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<(u32, u32)> {
		self.client
			.runtime_api()
			.cf_auction_parameters(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}
	fn cf_min_funding(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<NumberOrHex> {
		self.client
			.runtime_api()
			.cf_min_funding(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(Into::into)
	}
	fn cf_current_epoch(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u32> {
		self.client
			.runtime_api()
			.cf_current_epoch(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}
	fn cf_epoch_duration(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u32> {
		self.client
			.runtime_api()
			.cf_epoch_duration(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}
	fn cf_current_epoch_started_at(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<u32> {
		self.client
			.runtime_api()
			.cf_current_epoch_started_at(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}
	fn cf_authority_emission_per_block(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<NumberOrHex> {
		self.client
			.runtime_api()
			.cf_authority_emission_per_block(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(Into::into)
	}
	fn cf_backup_emission_per_block(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<NumberOrHex> {
		self.client
			.runtime_api()
			.cf_backup_emission_per_block(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(Into::into)
	}
	fn cf_flip_supply(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<(NumberOrHex, NumberOrHex)> {
		self.client
			.runtime_api()
			.cf_flip_supply(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(|(issuance, offchain)| (issuance.into(), offchain.into()))
	}
	fn cf_accounts(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<Vec<(state_chain_runtime::AccountId, String)>> {
		Ok(self
			.client
			.runtime_api()
			.cf_accounts(self.unwrap_or_best(at))
			.map_err(to_rpc_error)?
			.into_iter()
			.map(|(account_id, vanity_name_bytes)| {
				// we can use from_utf8_lossy here because we're guaranteed utf8 when we
				// save the vanity name on the chain
				(account_id, String::from_utf8_lossy(&vanity_name_bytes).into_owned())
			})
			.collect())
	}

	fn cf_account_info(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfo> {
		let api = self.client.runtime_api();

		let hash = self.unwrap_or_best(at);

		let balance = api.cf_account_flip_balance(hash, &account_id).map_err(to_rpc_error)?;

		Ok(
			match api
				.cf_account_role(hash, account_id.clone())
				.map_err(to_rpc_error)?
				.unwrap_or(AccountRole::Unregistered)
			{
				AccountRole::Unregistered => RpcAccountInfo::unregistered(balance),
				AccountRole::Broker => {
					let info = api.cf_broker_info(hash, account_id).map_err(to_rpc_error)?;

					RpcAccountInfo::broker(balance, info)
				},
				AccountRole::LiquidityProvider => {
					let info =
						api.cf_liquidity_provider_info(hash, account_id).map_err(to_rpc_error)?;

					RpcAccountInfo::lp(
						info,
						api.cf_network_environment(hash).map_err(to_rpc_error)?,
						balance,
					)
				},
				AccountRole::Validator => {
					let info = api.cf_validator_info(hash, &account_id).map_err(to_rpc_error)?;

					RpcAccountInfo::validator(info)
				},
			},
		)
	}

	fn cf_account_info_v2(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<RpcAccountInfoV2> {
		let account_info = self
			.client
			.runtime_api()
			.cf_validator_info(self.unwrap_or_best(at), &account_id)
			.map_err(to_rpc_error)?;

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
		})
	}

	fn cf_free_balances(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<any::AssetMap<U256>> {
		self.client
			.runtime_api()
			.cf_free_balances(self.unwrap_or_best(at), account_id)
			.map(|asset_map| asset_map.map(Into::into))
			.map_err(to_rpc_error)
	}

	fn cf_lp_total_balances(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<any::AssetMap<U256>> {
		self.client
			.runtime_api()
			.cf_lp_total_balances(self.unwrap_or_best(at), account_id)
			.map(|asset_map| asset_map.map(Into::into))
			.map_err(to_rpc_error)
	}

	fn cf_penalties(
		&self,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<Vec<(Offence, RpcPenalty)>> {
		Ok(self
			.client
			.runtime_api()
			.cf_penalties(self.unwrap_or_best(at))
			.map_err(to_rpc_error)?
			.iter()
			.map(|(offence, runtime_api_penalty)| {
				(
					*offence,
					RpcPenalty {
						reputation_points: runtime_api_penalty.reputation_points,
						suspension_duration_blocks: runtime_api_penalty.suspension_duration_blocks,
					},
				)
			})
			.collect())
	}
	fn cf_suspensions(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<RpcSuspensions> {
		self.client
			.runtime_api()
			.cf_suspensions(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}

	fn cf_generate_gov_key_call_hash(
		&self,
		call: Vec<u8>,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<GovCallHash> {
		self.client
			.runtime_api()
			.cf_generate_gov_key_call_hash(self.unwrap_or_best(at), call)
			.map_err(to_rpc_error)
	}

	fn cf_auction_state(&self, at: Option<<B as BlockT>::Hash>) -> RpcResult<RpcAuctionState> {
		let auction_state = self
			.client
			.runtime_api()
			.cf_auction_state(self.unwrap_or_best(at))
			.map_err(to_rpc_error)?;

		Ok(RpcAuctionState {
			blocks_per_epoch: auction_state.blocks_per_epoch,
			current_epoch_started_at: auction_state.current_epoch_started_at,
			redemption_period_as_percentage: auction_state.redemption_period_as_percentage,
			min_funding: auction_state.min_funding.into(),
			auction_size_range: auction_state.auction_size_range,
			min_active_bid: auction_state.min_active_bid.map(|bond| bond.into()),
		})
	}

	fn cf_pool_price(
		&self,
		from_asset: Asset,
		to_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolPriceV1>> {
		self.client
			.runtime_api()
			.cf_pool_price(self.unwrap_or_best(at), from_asset, to_asset)
			.map_err(to_rpc_error)
	}

	fn cf_pool_price_v2(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPriceV2> {
		let hash = self.unwrap_or_best(at);
		Ok(PoolPriceV2 {
			base_asset,
			quote_asset,
			price: self
				.client
				.runtime_api()
				.cf_pool_price_v2(hash, base_asset, quote_asset)
				.map_err(to_rpc_error)
				.and_then(|result| result.map_err(map_dispatch_error))?,
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
		self.client
			.runtime_api()
			.cf_pool_simulate_swap(
				self.unwrap_or_best(at),
				from_asset,
				to_asset,
				amount
					.try_into()
					.and_then(|amount| {
						if amount == 0 {
							Err("Swap input amount cannot be zero.")
						} else {
							Ok(amount)
						}
					})
					.map_err(|str| str_to_rpc_error(str))?,
				additional_orders.map(|additional_orders| {
					additional_orders
						.into_iter()
						.map(|additional_order| {
							match additional_order {
								SwapRateV2AdditionalOrder::LimitOrder {
									base_asset,
									quote_asset,
									side,
									tick,
									sell_amount,
								} => state_chain_runtime::runtime_apis::SimulateSwapAdditionalOrder::LimitOrder {
									base_asset,
									quote_asset,
									side,
									tick,
									sell_amount: sell_amount.unique_saturated_into(),
								}
							}
						})
						.collect()
				}),
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
			.map(|simulated_swap_info| RpcSwapOutputV2 {
				intermediary: simulated_swap_info.intermediary.map(Into::into),
				output: simulated_swap_info.output.into(),
				network_fee: RpcFee {
					asset: cf_primitives::STABLE_ASSET,
					amount: simulated_swap_info.network_fee.into(),
				},
				ingress_fee: RpcFee {
					asset: from_asset,
					amount: simulated_swap_info.ingress_fee.into(),
				},
				egress_fee: RpcFee {
					asset: to_asset,
					amount: simulated_swap_info.egress_fee.into(),
				},
			})
	}

	fn cf_pool_info(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolInfo> {
		self.client
			.runtime_api()
			.cf_pool_info(self.unwrap_or_best(at), base_asset, quote_asset)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_depth(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AskBidMap<UnidirectionalPoolDepth>> {
		self.client
			.runtime_api()
			.cf_pool_depth(self.unwrap_or_best(at), base_asset, quote_asset, tick_range)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_boost_pools_depth(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<BoostPoolDepth>> {
		self.client
			.runtime_api()
			.cf_boost_pools_depth(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}

	fn cf_pool_liquidity(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolLiquidity> {
		self.client
			.runtime_api()
			.cf_pool_liquidity(self.unwrap_or_best(at), base_asset, quote_asset)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPairsMap<Amount>> {
		self.client
			.runtime_api()
			.cf_required_asset_ratio_for_range_order(
				self.unwrap_or_best(at),
				base_asset,
				quote_asset,
				tick_range,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_orderbook(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		orders: u32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrderbook> {
		self.client
			.runtime_api()
			.cf_pool_orderbook(self.unwrap_or_best(at), base_asset, quote_asset, orders)
			.map_err(to_rpc_error)
			.and_then(|result| result.map(Into::into).map_err(map_dispatch_error))
	}

	fn cf_pool_orders(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		lp: Option<state_chain_runtime::AccountId>,
		filled_orders: Option<bool>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrders<state_chain_runtime::Runtime>> {
		self.client
			.runtime_api()
			.cf_pool_orders(
				self.unwrap_or_best(at),
				base_asset,
				quote_asset,
				lp,
				filled_orders.unwrap_or(false),
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_range_order_liquidity_value(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPairsMap<Amount>> {
		self.client
			.runtime_api()
			.cf_pool_range_order_liquidity_value(
				self.unwrap_or_best(at),
				base_asset,
				quote_asset,
				tick_range,
				liquidity,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_ingress_egress_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<IngressEgressEnvironment> {
		let runtime_api = &self.client.runtime_api();
		let hash = self.unwrap_or_best(at);

		let mut witness_safety_margins = HashMap::new();
		let mut channel_opening_fees = HashMap::new();

		for chain in ForeignChain::iter() {
			witness_safety_margins.insert(
				chain,
				runtime_api.cf_witness_safety_margin(hash, chain).map_err(to_rpc_error)?,
			);
			channel_opening_fees.insert(
				chain,
				runtime_api.cf_channel_opening_fee(hash, chain).map_err(to_rpc_error)?.into(),
			);
		}

		Ok(IngressEgressEnvironment {
			minimum_deposit_amounts: any::AssetMap::try_from_fn(|asset| {
				runtime_api
					.cf_min_deposit_amount(hash, asset)
					.map_err(to_rpc_error)
					.map(Into::into)
			})?,
			ingress_fees: any::AssetMap::try_from_fn(|asset| {
				runtime_api
					.cf_ingress_fee(hash, asset)
					.map_err(to_rpc_error)
					.map(|value| value.map(Into::into))
			})?,
			egress_fees: any::AssetMap::try_from_fn(|asset| {
				runtime_api
					.cf_egress_fee(hash, asset)
					.map_err(to_rpc_error)
					.map(|value| value.map(Into::into))
			})?,
			witness_safety_margins,
			egress_dust_limits: any::AssetMap::try_from_fn(|asset| {
				runtime_api
					.cf_egress_dust_limit(hash, asset)
					.map_err(to_rpc_error)
					.map(Into::into)
			})?,
			channel_opening_fees,
		})
	}

	fn cf_swapping_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<SwappingEnvironment> {
		let runtime_api = &self.client.runtime_api();
		let hash = self.unwrap_or_best(at);
		let swap_limits = runtime_api.cf_swap_limits(hash).map_err(to_rpc_error)?;
		Ok(SwappingEnvironment {
			maximum_swap_amounts: any::AssetMap::try_from_fn(|asset| {
				runtime_api
					.cf_max_swap_amount(hash, asset)
					.map_err(to_rpc_error)
					.map(|option| option.map(Into::into))
			})?,
			network_fee_hundredth_pips: NetworkFee::get(),
			swap_retry_delay_blocks: runtime_api
				.cf_swap_retry_delay_blocks(hash)
				.map_err(to_rpc_error)?,
			max_swap_retry_duration_blocks: swap_limits.max_swap_retry_duration_blocks,
			max_swap_request_duration_blocks: swap_limits.max_swap_request_duration_blocks,
		})
	}

	fn cf_funding_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<FundingEnvironment> {
		let runtime_api = &self.client.runtime_api();
		let hash = self.unwrap_or_best(at);

		Ok(FundingEnvironment {
			redemption_tax: runtime_api.cf_redemption_tax(hash).map_err(to_rpc_error)?.into(),
			minimum_funding_amount: runtime_api.cf_min_funding(hash).map_err(to_rpc_error)?.into(),
		})
	}

	fn cf_pools_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolsEnvironment> {
		Ok(PoolsEnvironment {
			fees: {
				let mut map = AssetMap::default();
				self.client
					.runtime_api()
					.cf_pools(self.unwrap_or_best(at))
					.map_err(to_rpc_error)?
					.iter()
					.for_each(|asset_pair| {
						map[asset_pair.base] = self
							.cf_pool_info(asset_pair.base, asset_pair.quote, at)
							.ok()
							.map(Into::into);
					});
				map
			},
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

	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer> {
		#[allow(deprecated)]
		self.client
			.runtime_api()
			.cf_current_compatibility_version(self.unwrap_or_best(None))
			.map_err(to_rpc_error)
	}

	fn cf_max_swap_amount(&self, asset: Asset) -> RpcResult<Option<AssetAmount>> {
		self.client
			.runtime_api()
			.cf_max_swap_amount(self.unwrap_or_best(None), asset)
			.map_err(to_rpc_error)
	}

	fn cf_subscribe_pool_price(
		&self,
		pending_sink: PendingSubscriptionSink,
		from_asset: Asset,
		to_asset: Asset,
	) {
		self.new_subscription(
			true,  /* only_on_changes */
			false, /* end_on_error */
			pending_sink,
			move |client, hash| client.runtime_api().cf_pool_price(hash, from_asset, to_asset),
		)
	}

	fn cf_subscribe_pool_price_v2(
		&self,
		pending_sink: PendingSubscriptionSink,
		base_asset: Asset,
		quote_asset: Asset,
	) {
		self.new_subscription(
			false, /* only_on_changes */
			true,  /* end_on_error */
			pending_sink,
			move |client, hash| {
				client
					.runtime_api()
					.cf_pool_price_v2(hash, base_asset, quote_asset)
					.map_err(to_rpc_error)
					.and_then(|result| result.map_err(map_dispatch_error))
					.map(|price| PoolPriceV2 { base_asset, quote_asset, price })
			},
		)
	}

	fn cf_subscribe_scheduled_swaps(
		&self,
		pending_sink: PendingSubscriptionSink,
		base_asset: Asset,
		quote_asset: Asset,
	) {
		// Check that the requested pool exists:
		let Ok(Ok(_)) = self.client.runtime_api().cf_pool_info(
			self.client.info().best_hash,
			base_asset,
			quote_asset,
		) else {
			return;
		};

		self.new_subscription(
			false, /* only_on_changes */
			true,  /* end_on_error */
			pending_sink,
			move |client, hash| {
				Ok::<_, ApiError>(SwapResponse {
					swaps: client
						.runtime_api()
						.cf_scheduled_swaps(hash, base_asset, quote_asset)?
						.into_iter()
						.map(|(swap, execute_at)| ScheduledSwap::new(swap, execute_at))
						.collect(),
				})
			},
		);
	}

	fn cf_scheduled_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<ScheduledSwap>> {
		// Check that the requested pool exists:
		self.client
			.runtime_api()
			.cf_pool_info(self.client.info().best_hash, base_asset, quote_asset)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))?;

		Ok(self
			.client
			.runtime_api()
			.cf_scheduled_swaps(self.unwrap_or_best(at), base_asset, quote_asset)
			.map_err(to_rpc_error)?
			.into_iter()
			.map(|(swap, execute_at)| ScheduledSwap::new(swap, execute_at))
			.collect())
	}

	fn cf_subscribe_prewitness_swaps(
		&self,
		pending_sink: PendingSubscriptionSink,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
	) {
		self.new_subscription(
			false, /* only_on_changes */
			true,  /* end_on_error */
			pending_sink,
			move |client, hash| {
				Ok::<_, ErrorObjectOwned>(RpcPrewitnessedSwap {
					base_asset,
					quote_asset,
					side,
					amounts: client
						.runtime_api()
						.cf_prewitness_swaps(hash, base_asset, quote_asset, side)
						.map_err(to_rpc_error)?
						.into_iter()
						.map(|s| s.into())
						.collect(),
				})
			},
		)
	}

	fn cf_prewitness_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcPrewitnessedSwap> {
		Ok(RpcPrewitnessedSwap {
			base_asset,
			quote_asset,
			side,
			amounts: self
				.client
				.runtime_api()
				.cf_prewitness_swaps(self.unwrap_or_best(at), base_asset, quote_asset, side)
				.map_err(to_rpc_error)?
				.into_iter()
				.map(|s| s.into())
				.collect(),
		})
	}

	fn cf_subscribe_lp_order_fills(
		&self,
		sink: PendingSubscriptionSink,
	) {
		self.new_subscription_with_state(
			false, /* only_on_changes */
			true,  /* end_on_error */
			sink,
			|client, hash, prev_pools| {
				let pools = StorageQueryApi::new(client)
					.collect_from_storage_map::<pallet_cf_pools::Pools<_>, _, _, _>(hash)?;

				let fills = prev_pools
					.map(|prev_pools| {
						let pools_events =
							client.runtime_api().cf_lp_events(hash).map_err(to_rpc_error)?;

						RpcResult::Ok(
							order_fills::order_fills_from_block_updates(
								prev_pools,
								&pools,
								pools_events,
							),
						)
					})
					.transpose()?
					.unwrap_or_default();

				RpcResult::Ok((fills, pools))
			},
		)
	}

	fn cf_supported_assets(&self) -> RpcResult<Vec<Asset>> {
		Ok(Asset::all().collect())
	}

	fn cf_failed_call_ethereum(
		&self,
		broadcast_id: BroadcastId,
	) -> RpcResult<Option<<cf_chains::Ethereum as Chain>::Transaction>> {
		self.client
			.runtime_api()
			.cf_failed_call_ethereum(self.unwrap_or_best(None), broadcast_id)
			.map_err(to_rpc_error)
	}

	fn cf_failed_call_arbitrum(
		&self,
		broadcast_id: BroadcastId,
	) -> RpcResult<Option<<cf_chains::Arbitrum as Chain>::Transaction>> {
		self.client
			.runtime_api()
			.cf_failed_call_arbitrum(self.unwrap_or_best(None), broadcast_id)
			.map_err(to_rpc_error)
	}

	fn cf_witness_count(
		&self,
		hash: state_chain_runtime::Hash,
		epoch_index: Option<EpochIndex>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<FailingWitnessValidators>> {
		self.client
			.runtime_api()
			.cf_witness_count(
				self.unwrap_or_best(at),
				pallet_cf_witnesser::CallHash(hash.into()),
				epoch_index,
			)
			.map_err(to_rpc_error)
	}

	fn cf_boost_pool_details(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolDetailsResponse> {
		execute_for_all_or_one_asset(asset, |asset| {
			self.client
				.runtime_api()
				.cf_boost_pool_details(self.unwrap_or_best(at), asset)
				.map(|details_for_each_pool| {
					details_for_each_pool
						.into_iter()
						.map(|(tier, details)| BoostPoolDetailsRpc::new(asset, tier, details))
						.collect()
				})
				.map_err(to_rpc_error)
		})
	}

	fn cf_boost_pool_pending_fees(
		&self,
		asset: Option<Asset>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<BoostPoolFeesResponse> {
		execute_for_all_or_one_asset(asset, |asset| {
			self.client
				.runtime_api()
				.cf_boost_pool_details(self.unwrap_or_best(at), asset)
				.map(|details_for_each_pool| {
					details_for_each_pool
						.into_iter()
						.map(|(fee_tier, details)| BoostPoolFeesRpc::new(asset, fee_tier, details))
						.collect()
				})
				.map_err(to_rpc_error)
		})
	}

	fn cf_safe_mode_statuses(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RuntimeSafeMode> {
		self.client
			.runtime_api()
			.cf_safe_mode_statuses(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}

	fn cf_available_pools(&self, at: Option<Hash>) -> RpcResult<Vec<PoolPairsMap<Asset>>> {
		self.client
			.runtime_api()
			.cf_pools(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
	}

	fn cf_solana_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		let runtime_api = self.client.runtime_api();
		ElectoralRuntimeApi::<_, SolanaInstance>::cf_electoral_data(
			&*runtime_api,
			self.unwrap_or_best(at),
			validator,
		)
		.map_err(to_rpc_error)
	}

	fn cf_solana_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		let runtime_api = self.client.runtime_api();
		ElectoralRuntimeApi::<_, SolanaInstance>::cf_filter_votes(
			&*runtime_api,
			self.unwrap_or_best(at),
			validator,
			proposed_votes,
		)
		.map_err(to_rpc_error)
	}
}

impl<C, B> CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ HeaderBackend<B>
		+ BlockchainEvents<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn new_subscription<
		T: Serialize + Send + Clone + Eq + 'static,
		E: std::error::Error + Send + Sync + 'static,
		F: Fn(&C, state_chain_runtime::Hash) -> Result<T, E> + Send + Clone + 'static,
	>(
		&self,
		only_on_changes: bool,
		end_on_error: bool,
		sink: PendingSubscriptionSink,
		f: F,
	) {
		self.new_subscription_with_state(
			only_on_changes,
			end_on_error,
			sink,
			move |client, hash, _state| f(client, hash).map(|res| (res, ())),
		)
	}

	/// The subscription will return the first value immediately and then either return new values
	/// only when it changes, or every new block. Note in both cases this can skip blocks. Also this
	/// subscription can either filter out, or end the stream if the provided async closure returns
	/// an error.
	fn new_subscription_with_state<
		T: Serialize + Send + Clone + Eq + 'static,
		E: std::error::Error + Send + Sync + 'static,
		// State to carry forward between calls to the closure.
		S: 'static + Clone + Send,
		F: Fn(&C, state_chain_runtime::Hash, Option<&S>) -> Result<(T, S), E> + Send + Clone + 'static,
	>(
		&self,
		only_on_changes: bool,
		end_on_error: bool,
		pending_sink: PendingSubscriptionSink,
		f: F,
	) {
		use futures::{stream::StreamExt, FutureExt};

		let info = self.client.info();

		let (initial_item, initial_state) = match f(&self.client, info.best_hash, None) {
			Ok(initial) => initial,
			Err(e) => {
				let _ = pending_sink.reject(
					sc_rpc_api::state::error::Error::Client(Box::new(to_rpc_error(e))),
				);
				return;
			},
		};

		let stream = futures::stream::iter(std::iter::once(Ok(BlockUpdate {
			block_hash: info.best_hash,
			block_number: info.best_number,
			data: initial_item.clone(),
		})))
		.chain(
			self.client
				.import_notification_stream()
				.filter(|n| futures::future::ready(n.is_new_best))
				.filter_map({
					let client = self.client.clone();

					let mut previous_item = initial_item;
					let mut previous_state = initial_state;

					move |n| {
						futures::future::ready(match f(&client, n.hash, Some(&previous_state)) {
							Ok((new_item, new_state))
								if !only_on_changes || new_item != previous_item =>
							{
								previous_item = new_item.clone();
								previous_state = new_state;
								Some(Ok(BlockUpdate {
									block_hash: n.hash,
									block_number: *n.header.number(),
									data: new_item,
								}))
							},
							Err(error) if end_on_error => Some(Err(to_rpc_error(error))),
							_ => None,
						})
					}
				}),
		);

		self.executor.spawn(
			"cf-rpc-update-subscription",
			Some("rpc"),
			sc_rpc::utils::pipe_from_stream(pending_sink, stream).boxed(),
		);
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

#[cfg(test)]
mod test {
	use std::collections::BTreeSet;

	use super::*;
	use cf_chains::assets::sol;
	use cf_primitives::{
		chains::assets::{any, arb, btc, dot, eth},
		FLIPPERINOS_PER_FLIP,
	};
	use sp_core::H160;
	use sp_runtime::AccountId32;
	use state_chain_runtime::runtime_apis::OwedAmount;

	/*
		changing any of these serialization tests signifies a breaking change in the
		API. please make sure to get approval from the product team before merging
		any changes that break a serialization test.

		if approval is received and a new breaking change is introduced, please
		stale the review and get a new review from someone on product.
	*/

	#[test]
	fn test_no_account_serialization() {
		insta::assert_snapshot!(serde_json::to_value(RpcAccountInfo::unregistered(0)).unwrap());
	}

	#[test]
	fn test_broker_serialization() {
		insta::assert_snapshot!(serde_json::to_value(RpcAccountInfo::broker(
			0,
			BrokerInfo {
				earned_fees: vec![
					(Asset::Eth, 0),
					(Asset::Btc, 0),
					(Asset::Flip, 1000000000000000000),
					(Asset::Usdc, 0),
					(Asset::Usdt, 0),
					(Asset::Dot, 0),
					(Asset::ArbEth, 0),
					(Asset::ArbUsdc, 0),
					(Asset::Sol, 0),
					(Asset::SolUsdc, 0),
				]
			}
		))
		.unwrap());
	}

	#[test]
	fn test_lp_serialization() {
		let lp = RpcAccountInfo::lp(
			LiquidityProviderInfo {
				refund_addresses: vec![
					(
						ForeignChain::Ethereum,
						Some(cf_chains::ForeignChainAddress::Eth(H160::from([1; 20]))),
					),
					(
						ForeignChain::Polkadot,
						Some(cf_chains::ForeignChainAddress::Dot(Default::default())),
					),
					(ForeignChain::Bitcoin, None),
					(
						ForeignChain::Arbitrum,
						Some(cf_chains::ForeignChainAddress::Arb(H160::from([2; 20]))),
					),
					(ForeignChain::Solana, None),
				],
				balances: vec![
					(Asset::Eth, u128::MAX),
					(Asset::Btc, 0),
					(Asset::Flip, u128::MAX / 2),
					(Asset::Usdc, 0),
					(Asset::Usdt, 0),
					(Asset::Dot, 0),
					(Asset::ArbEth, 1),
					(Asset::ArbUsdc, 2),
					(Asset::Sol, 3),
					(Asset::SolUsdc, 4),
				],
				earned_fees: any::AssetMap {
					eth: eth::AssetMap {
						eth: 0u32.into(),
						flip: u64::MAX.into(),
						usdc: (u64::MAX / 2 - 1).into(),
						usdt: 0u32.into(),
					},
					btc: btc::AssetMap { btc: 0u32.into() },
					dot: dot::AssetMap { dot: 0u32.into() },
					arb: arb::AssetMap { eth: 1u32.into(), usdc: 2u32.into() },
					sol: sol::AssetMap { sol: 2u32.into(), usdc: 4u32.into() },
				},
				boost_balances: any::AssetMap {
					btc: btc::AssetMap {
						btc: vec![LiquidityProviderBoostPoolInfo {
							fee_tier: 5,
							total_balance: 100_000_000,
							available_balance: 50_000_000,
							in_use_balance: 50_000_000,
							is_withdrawing: false,
						}],
					},
					..Default::default()
				},
			},
			cf_primitives::NetworkEnvironment::Mainnet,
			0,
		);

		insta::assert_snapshot!(serde_json::to_value(lp).unwrap());
	}

	#[test]
	fn test_validator_serialization() {
		let validator = RpcAccountInfo::validator(ValidatorInfo {
			balance: FLIPPERINOS_PER_FLIP,
			bond: FLIPPERINOS_PER_FLIP,
			last_heartbeat: 0,
			reputation_points: 0,
			keyholder_epochs: vec![123],
			is_current_authority: true,
			is_bidding: false,
			is_current_backup: false,
			is_online: true,
			is_qualified: true,
			bound_redeem_address: Some(H160::from([1; 20])),
			apy_bp: Some(100u32),
			restricted_balances: BTreeMap::from_iter(vec![(
				H160::from([1; 20]),
				FLIPPERINOS_PER_FLIP,
			)]),
		});

		insta::assert_snapshot!(serde_json::to_value(validator).unwrap());
	}

	#[test]
	fn test_environment_serialization() {
		let env = RpcEnvironment {
			swapping: SwappingEnvironment {
				maximum_swap_amounts: any::AssetMap {
					eth: eth::AssetMap {
						eth: Some(0u32.into()),
						flip: None,
						usdc: Some((u64::MAX / 2 - 1).into()),
						usdt: None,
					},
					btc: btc::AssetMap { btc: Some(0u32.into()) },
					dot: dot::AssetMap { dot: None },
					arb: arb::AssetMap { eth: None, usdc: Some(0u32.into()) },
					sol: sol::AssetMap { sol: None, usdc: None },
				},
				network_fee_hundredth_pips: Permill::from_percent(100),
				swap_retry_delay_blocks: 5,
				max_swap_retry_duration_blocks: 600,
				max_swap_request_duration_blocks: 14400,
			},
			ingress_egress: IngressEgressEnvironment {
				minimum_deposit_amounts: any::AssetMap {
					eth: eth::AssetMap {
						eth: 0u32.into(),
						flip: u64::MAX.into(),
						usdc: (u64::MAX / 2 - 1).into(),
						usdt: 0u32.into(),
					},
					btc: btc::AssetMap { btc: 0u32.into() },
					dot: dot::AssetMap { dot: 0u32.into() },
					arb: arb::AssetMap { eth: 0u32.into(), usdc: u64::MAX.into() },
					sol: sol::AssetMap { sol: 0u32.into(), usdc: 0u32.into() },
				},
				ingress_fees: any::AssetMap {
					eth: eth::AssetMap {
						eth: Some(0u32.into()),
						flip: Some(AssetAmount::MAX.into()),
						usdc: None,
						usdt: None,
					},
					btc: btc::AssetMap { btc: Some(0u32.into()) },
					dot: dot::AssetMap { dot: Some((u64::MAX / 2 - 1).into()) },
					arb: arb::AssetMap { eth: Some(0u32.into()), usdc: None },
					sol: sol::AssetMap { sol: Some(0u32.into()), usdc: None },
				},
				egress_fees: any::AssetMap {
					eth: eth::AssetMap {
						eth: Some(0u32.into()),
						usdc: None,
						flip: Some(AssetAmount::MAX.into()),
						usdt: None,
					},
					btc: btc::AssetMap { btc: Some(0u32.into()) },
					dot: dot::AssetMap { dot: Some((u64::MAX / 2 - 1).into()) },
					arb: arb::AssetMap { eth: Some(0u32.into()), usdc: None },
					sol: sol::AssetMap { sol: Some(1u32.into()), usdc: None },
				},
				witness_safety_margins: HashMap::from([
					(ForeignChain::Bitcoin, Some(3u64)),
					(ForeignChain::Ethereum, Some(3u64)),
					(ForeignChain::Polkadot, None),
					(ForeignChain::Arbitrum, None),
					(ForeignChain::Solana, None),
				]),
				egress_dust_limits: any::AssetMap {
					eth: eth::AssetMap {
						eth: 0u32.into(),
						usdc: (u64::MAX / 2 - 1).into(),
						flip: AssetAmount::MAX.into(),
						usdt: 0u32.into(),
					},
					btc: btc::AssetMap { btc: 0u32.into() },
					dot: dot::AssetMap { dot: 0u32.into() },
					arb: arb::AssetMap { eth: 0u32.into(), usdc: u64::MAX.into() },
					sol: sol::AssetMap { sol: 0u32.into(), usdc: 0u32.into() },
				},
				channel_opening_fees: HashMap::from([
					(ForeignChain::Bitcoin, 0u32.into()),
					(ForeignChain::Ethereum, 1000u32.into()),
					(ForeignChain::Polkadot, 1000u32.into()),
					(ForeignChain::Arbitrum, 1000u32.into()),
					(ForeignChain::Solana, 1000u32.into()),
				]),
			},
			funding: FundingEnvironment {
				redemption_tax: 0u32.into(),
				minimum_funding_amount: 0u32.into(),
			},
			pools: {
				let pool_info: RpcPoolInfo = PoolInfo {
					limit_order_fee_hundredth_pips: 0,
					range_order_fee_hundredth_pips: 100,
					range_order_total_fees_earned: Default::default(),
					limit_order_total_fees_earned: Default::default(),
					range_total_swap_inputs: Default::default(),
					limit_total_swap_inputs: Default::default(),
				}
				.into();
				PoolsEnvironment {
					fees: any::AssetMap {
						eth: eth::AssetMap {
							eth: None,
							usdc: None,
							flip: Some(pool_info),
							usdt: Some(pool_info),
						},
						btc: btc::AssetMap { btc: Some(pool_info) },
						dot: dot::AssetMap { dot: Some(pool_info) },
						arb: arb::AssetMap { eth: Some(pool_info), usdc: Some(pool_info) },
						sol: sol::AssetMap { sol: Some(pool_info), usdc: None },
					},
				}
			},
		};

		insta::assert_snapshot!(serde_json::to_value(env).unwrap());
	}

	#[test]
	fn test_boost_depth_serialization() {
		let val: BoostPoolDepthResponse = vec![
			BoostPoolDepth {
				asset: Asset::Flip,
				tier: 10,
				available_amount: 1_000_000_000 * FLIPPERINOS_PER_FLIP,
			},
			BoostPoolDepth { asset: Asset::Flip, tier: 30, available_amount: 0 },
		];
		insta::assert_json_snapshot!(val);
	}

	const ID_1: AccountId32 = AccountId32::new([1; 32]);
	const ID_2: AccountId32 = AccountId32::new([2; 32]);

	fn boost_details_1() -> BoostPoolDetails {
		BoostPoolDetails {
			available_amounts: BTreeMap::from([(ID_1.clone(), 10_000)]),
			pending_boosts: BTreeMap::from([
				(
					0,
					BTreeMap::from([
						(ID_1.clone(), OwedAmount { total: 200, fee: 10 }),
						(ID_2.clone(), OwedAmount { total: 2_000, fee: 100 }),
					]),
				),
				(1, BTreeMap::from([(ID_1.clone(), OwedAmount { total: 1_000, fee: 50 })])),
			]),
			pending_withdrawals: Default::default(),
		}
	}

	fn boost_details_2() -> BoostPoolDetails {
		BoostPoolDetails {
			available_amounts: BTreeMap::from([]),
			pending_boosts: BTreeMap::from([(
				0,
				BTreeMap::from([
					(ID_1.clone(), OwedAmount { total: 1_000, fee: 50 }),
					(ID_2.clone(), OwedAmount { total: 2_000, fee: 100 }),
				]),
			)]),
			pending_withdrawals: BTreeMap::from([
				(ID_1.clone(), BTreeSet::from([0])),
				(ID_2.clone(), BTreeSet::from([0])),
			]),
		}
	}

	#[test]
	fn test_boost_details_serialization() {
		let val: BoostPoolDetailsResponse = vec![
			BoostPoolDetailsRpc::new(Asset::ArbEth, 10, boost_details_1()),
			BoostPoolDetailsRpc::new(Asset::Btc, 30, boost_details_2()),
		];

		insta::assert_json_snapshot!(val);
	}

	#[test]
	fn test_boost_fees_serialization() {
		let val: BoostPoolFeesResponse =
			vec![BoostPoolFeesRpc::new(Asset::Btc, 10, boost_details_1())];

		insta::assert_json_snapshot!(val);
	}

	#[test]
	fn test_swap_output_serialization() {
		insta::assert_snapshot!(serde_json::to_value(RpcSwapOutputV2 {
			output: 1_000_000_000_000_000_000u128.into(),
			intermediary: Some(1_000_000u128.into()),
			network_fee: RpcFee { asset: Asset::Usdc, amount: 1_000u128.into() },
			ingress_fee: RpcFee { asset: Asset::Flip, amount: 500u128.into() },
			egress_fee: RpcFee { asset: Asset::Eth, amount: 1_000_000u128.into() },
		})
		.unwrap());
	}
}