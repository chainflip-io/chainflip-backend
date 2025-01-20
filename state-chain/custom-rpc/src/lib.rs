use crate::{boost_pool_rpc::BoostPoolFeesRpc, monitoring::RpcEpochStateV2};
use boost_pool_rpc::BoostPoolDetailsRpc;
use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount as AmmAmount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{AddressString, ForeignChainAddressHumanreadable, ToHumanreadableAddress},
	dot::PolkadotAccountId,
	eth::Address as EthereumAddress,
	sol::SolAddress,
	CcmChannelMetadata, Chain, VaultSwapExtraParametersRpc, MAX_CCM_MSG_LENGTH,
};
use cf_primitives::{
	chains::assets::any::{self, AssetMap},
	AccountId, AccountRole, AffiliateShortId, Affiliates, Asset, AssetAmount, BasisPoints,
	BlockNumber, BroadcastId, DcaParameters, EpochIndex, ForeignChain, NetworkEnvironment, SemVer,
	SwapId, SwapRequestId,
};
use cf_utilities::rpc::NumberOrHex;
use core::ops::Range;
use futures::{stream, stream::StreamExt, FutureExt};
use jsonrpsee::{
	core::async_trait,
	proc_macros::rpc,
	types::{
		error::{ErrorObject, ErrorObjectOwned},
		ErrorCode,
	},
	PendingSubscriptionSink, RpcModule,
};
use order_fills::OrderFills;
use pallet_cf_governance::GovCallHash;
use pallet_cf_pools::{
	AskBidMap, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders, PoolPriceV1,
	UnidirectionalPoolDepth,
};
use pallet_cf_swapping::SwapLegInfo;
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, BlockchainEvents, ExecutorProvider,
	HeaderBackend, StorageProvider,
};
use sc_rpc_spec_v2::chain_head::{
	api::ChainHeadApiServer, ChainHead, ChainHeadConfig, FollowEvent,
};
use serde::{Deserialize, Serialize};
use sp_api::{ApiError, CallApiAt};
use sp_core::U256;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto},
	Percent, Permill,
};
use sp_state_machine::InspectState;
use state_chain_runtime::{
	chainflip::{BlockUpdate, Offence},
	constants::common::TX_FEE_MULTIPLIER,
	monitoring_apis::{
		ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, ExternalChainsBlockHeight,
		FeeImbalance, FlipSupply, LastRuntimeUpgradeInfo, MonitoringDataV2, OpenDepositChannels,
		PendingBroadcasts, PendingTssCeremonies, RedemptionsInfo, SolanaNonces,
	},
	runtime_apis::{
		AuctionState, BoostPoolDepth, BoostPoolDetails, BrokerInfo, CcmData, ChainAccounts,
		CustomRuntimeApi, DispatchErrorWithMessage, ElectoralRuntimeApi, FailingWitnessValidators,
		FeeTypes, LiquidityProviderBoostPoolInfo, LiquidityProviderInfo, RuntimeApiPenalty,
		SimulatedSwapInformation, TransactionScreeningEvents, ValidatorInfo, VaultSwapDetails,
	},
	safe_mode::RuntimeSafeMode,
	Hash, NetworkFee, SolanaInstance,
};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	marker::PhantomData,
	sync::Arc,
};

pub mod monitoring;
pub mod order_fills;

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
	pub epoch: RpcEpochStateV2,
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
impl From<MonitoringDataV2> for RpcMonitoringData {
	fn from(monitoring_data: MonitoringDataV2) -> Self {
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
		bond: NumberOrHex,
		earned_fees: any::AssetMap<NumberOrHex>,
		affiliates: Vec<(AffiliateShortId, AccountId)>,
		btc_vault_deposit_address: Option<String>,
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

	fn broker(broker_info: BrokerInfo, balance: u128) -> Self {
		Self::Broker {
			flip_balance: balance.into(),
			bond: broker_info.bond.into(),
			btc_vault_deposit_address: broker_info.btc_vault_deposit_address,
			earned_fees: cf_chains::assets::any::AssetMap::from_iter_or_default(
				broker_info
					.earned_fees
					.iter()
					.map(|(asset, balance)| (*asset, (*balance).into())),
			),
			affiliates: broker_info.affiliates,
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
	epoch_duration: u32,
	current_epoch_started_at: u32,
	redemption_period_as_percentage: u8,
	min_funding: NumberOrHex,
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
	minimum_chunk_size: any::AssetMap<NumberOrHex>,
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
		network_fee_deduction_percent: Percent,
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
	) -> RpcResult<PoolInfo>;
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
	#[subscription(name = "subscribe_prewitness_swaps", item = BlockUpdate<RpcPrewitnessedSwap>)]
	async fn cf_subscribe_prewitness_swaps(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
	);

	// Subscribe to a stream that on every block produces a list of all scheduled/pending
	// swaps in the base_asset/quote_asset pool, including any "implicit" half-swaps (as a
	// part of a swap involving two pools)
	#[subscription(name = "subscribe_scheduled_swaps", item = BlockUpdate<SwapResponse>)]
	async fn cf_subscribe_scheduled_swaps(&self, base_asset: Asset, quote_asset: Asset);

	#[subscription(name = "subscribe_lp_order_fills", item = BlockUpdate<OrderFills>)]
	async fn cf_subscribe_lp_order_fills(&self);

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
		retry_duration: BlockNumber,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<()>;

	#[method(name = "get_vault_swap_details")]
	fn cf_get_vault_swap_details(
		&self,
		broker: state_chain_runtime::AccountId,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<state_chain_runtime::AccountId>>,
		dca_parameters: Option<DcaParameters>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultSwapDetails<AddressString>>;

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
	fn cf_get_affiliates(
		&self,
		broker: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(AffiliateShortId, state_chain_runtime::AccountId)>>;
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B, BE> {
	pub client: Arc<C>,
	pub backend: Arc<BE>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
	pub _phantom: PhantomData<B>,
}

impl<C, B, BE> CustomRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + HeaderBackend<B>,
{
	fn unwrap_or_best(&self, from_rpc: Option<<B as BlockT>::Hash>) -> B::Hash {
		from_rpc.unwrap_or_else(|| self.client.info().best_hash)
	}
}

impl<C, B, BE> CustomRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + HeaderBackend<B> + sp_api::ProvideRuntimeApi<B>,
{
	fn with_runtime_api<E, R>(
		&self,
		at: Option<Hash>,
		f: impl FnOnce(&C::Api, Hash) -> Result<R, E>,
	) -> RpcResult<R>
	where
		CfApiError: From<E>,
	{
		Ok(f(&*self.client.runtime_api(), self.unwrap_or_best(at))?)
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
		Ok(self.0.state_at(hash)?.inspect_state(f))
	}
}

#[derive(thiserror::Error, Debug)]
pub enum CfApiError {
	#[error(transparent)]
	ClientError(#[from] jsonrpsee::core::client::Error),
	#[error("{0:?}")]
	DispatchError(#[from] DispatchErrorWithMessage),
	#[error("{0:?}")]
	RuntimeApiError(#[from] ApiError),
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
	#[error(transparent)]
	OtherError(#[from] anyhow::Error),
}
pub type RpcResult<T> = Result<T, CfApiError>;

fn internal_error(error: impl core::fmt::Debug) -> ErrorObjectOwned {
	log::error!(target: "cf_rpc", "Internal error: {:?}", error);
	ErrorObject::owned(
		ErrorCode::InternalError.code(),
		"Internal error while processing request.",
		None::<()>,
	)
}
fn call_error(error: impl Into<Box<dyn core::error::Error + Sync + Send>>) -> ErrorObjectOwned {
	let error = error.into();
	log::debug!(target: "cf_rpc", "Call error: {}", error);
	ErrorObject::owned(ErrorCode::InternalError.code(), format!("{error}"), None::<()>)
}

impl From<CfApiError> for ErrorObjectOwned {
	fn from(error: CfApiError) -> Self {
		match error {
			CfApiError::ClientError(client_error) => match client_error {
				jsonrpsee::core::client::Error::Call(obj) => obj,
				other => internal_error(other),
			},
			CfApiError::DispatchError(dispatch_error) => match dispatch_error {
				DispatchErrorWithMessage::Module(message) |
				DispatchErrorWithMessage::RawMessage(message) => match std::str::from_utf8(&message) {
					Ok(message) => call_error(std::format!("DispatchError: {message}")),
					Err(error) =>
						internal_error(format!("Unable to decode Module Error Message: {error}")),
				},
				DispatchErrorWithMessage::Other(error) =>
					internal_error(format!("Unable to decode DispatchError: {error:?}")),
			},
			CfApiError::RuntimeApiError(error) => match error {
				ApiError::Application(error) => call_error(format!("Application error: {error}")),
				ApiError::UnknownBlock(error) => call_error(format!("Unknown block: {error}")),
				other => internal_error(format!("Unexpected ApiError: {other}")),
			},
			CfApiError::ErrorObject(object) => object,
			CfApiError::OtherError(error) => internal_error(error),
		}
	}
}

#[macro_export]
macro_rules! pass_through {
	($( $name:ident ( $( $arg:ident: $argt:ty ),* $(,)? ) -> $result_type:ty $([map: $mapping:expr])? ),+ $(,)?) => {
		$(
			fn $name(&self, $( $arg: $argt, )* at: Option<state_chain_runtime::Hash>,) -> RpcResult<$result_type> {
				self.with_runtime_api(at, |api, hash| api.$name(hash, $($arg),* ))
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
					self.with_runtime_api(at, |api, hash| api.$name(hash, $($arg),* ))
						$(.map($mapping))?
				)
			}
		)+
	};
}

fn flatten_into_error<R, E1, E2>(res: Result<Result<R, E1>, E2>) -> Result<R, E2>
where
	E2: From<E1>,
{
	match res.map(|inner| inner.map_err(Into::into)) {
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
	C::Api: CustomRuntimeApi<B> + ElectoralRuntimeApi<B, SolanaInstance>,
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
		cf_auction_state() -> RpcAuctionState [map: Into::into],
		cf_safe_mode_statuses() -> RuntimeSafeMode,
		cf_failed_call_ethereum(broadcast_id: BroadcastId) -> Option<<cf_chains::Ethereum as Chain>::Transaction>,
		cf_failed_call_arbitrum(broadcast_id: BroadcastId) -> Option<<cf_chains::Arbitrum as Chain>::Transaction>,
		cf_boost_pools_depth() -> Vec<BoostPoolDepth>,
		cf_pool_price(from_asset: Asset, to_asset: Asset) -> Option<PoolPriceV1>,
		cf_get_open_deposit_channels(account_id: Option<state_chain_runtime::AccountId>) -> ChainAccounts,
		cf_get_affiliates(broker: state_chain_runtime::AccountId) -> Vec<(AffiliateShortId, state_chain_runtime::AccountId)>,
	}

	pass_through_and_flatten! {
		cf_required_asset_ratio_for_range_order(base_asset: Asset, quote_asset: Asset, tick_range: Range<Tick>) -> PoolPairsMap<AmmAmount>,
		cf_pool_orderbook(base_asset: Asset, quote_asset: Asset, orders: u32) -> PoolOrderbook,
		cf_pool_info(base_asset: Asset, quote_asset: Asset) -> PoolInfo,
		cf_pool_depth(base_asset: Asset, quote_asset: Asset, tick_range: Range<Tick>) -> AskBidMap<UnidirectionalPoolDepth>,
		cf_pool_liquidity(base_asset: Asset, quote_asset: Asset) -> PoolLiquidity,
		cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> PoolPairsMap<AmmAmount>,
		cf_validate_dca_params(number_of_chunks: u32, chunk_interval: u32) -> (),
		cf_validate_refund_params(retry_duration: BlockNumber) -> (),
	}

	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer> {
		#[allow(deprecated)]
		self.with_runtime_api(None, |api, hash| api.cf_current_compatibility_version(hash))
	}

	fn cf_max_swap_amount(&self, asset: Asset) -> RpcResult<Option<AssetAmount>> {
		self.with_runtime_api(None, |api, hash| api.cf_max_swap_amount(hash, asset))
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
		self.with_runtime_api(at, |api, block_hash| {
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
		flatten_into_error(self.with_runtime_api(at, |api, hash| {
			api.cf_pool_orders(hash, base_asset, quote_asset, lp, filled_orders.unwrap_or_default())
		}))
	}
	fn cf_pool_price_v2(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPriceV2> {
		self.with_runtime_api(at, |api, hash| {
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
	) -> RpcResult<RpcAccountInfo> {
		self.with_runtime_api(at, |api, hash| {
			let balance = api.cf_account_flip_balance(hash, &account_id)?;

			Ok::<_, CfApiError>(
				match api
					.cf_account_role(hash, account_id.clone())?
					.unwrap_or(AccountRole::Unregistered)
				{
					AccountRole::Unregistered => RpcAccountInfo::unregistered(balance),
					AccountRole::Broker => {
						let info = api.cf_broker_info(hash, account_id)?;

						RpcAccountInfo::broker(info, balance)
					},
					AccountRole::LiquidityProvider => {
						let info = api.cf_liquidity_provider_info(hash, account_id)?;

						RpcAccountInfo::lp(info, api.cf_network_environment(hash)?, balance)
					},
					AccountRole::Validator => {
						let info = api.cf_validator_info(hash, &account_id)?;

						RpcAccountInfo::validator(info)
					},
				},
			)
		})
	}

	fn cf_account_info_v2(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<RpcAccountInfoV2> {
		let account_info =
			self.with_runtime_api(at, |api, hash| api.cf_validator_info(hash, &account_id))?;

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
			at,
		)
		.map(Into::into)
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
				return Err(CfApiError::ErrorObject(ErrorObject::owned(
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
					} =>
						state_chain_runtime::runtime_apis::SimulateSwapAdditionalOrder::LimitOrder {
							base_asset,
							quote_asset,
							side,
							tick,
							sell_amount: sell_amount.unique_saturated_into(),
						},
				})
				.collect()
		});
		self.with_runtime_api(at, |api, hash| {
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
		self.with_runtime_api(at, |api, hash| {
			let mut witness_safety_margins = HashMap::new();
			let mut channel_opening_fees = HashMap::new();

			for chain in ForeignChain::iter() {
				witness_safety_margins.insert(chain, api.cf_witness_safety_margin(hash, chain)?);
				channel_opening_fees.insert(chain, api.cf_channel_opening_fee(hash, chain)?.into());
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
			})
		})
	}

	fn cf_swapping_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<SwappingEnvironment> {
		self.with_runtime_api(at, |api, hash| {
			let swap_limits = api.cf_swap_limits(hash)?;
			Ok::<_, CfApiError>(SwappingEnvironment {
				maximum_swap_amounts: any::AssetMap::try_from_fn(|asset| {
					api.cf_max_swap_amount(hash, asset).map(|option| option.map(Into::into))
				})?,
				network_fee_hundredth_pips: NetworkFee::get(),
				swap_retry_delay_blocks: api.cf_swap_retry_delay_blocks(hash)?,
				max_swap_retry_duration_blocks: swap_limits.max_swap_retry_duration_blocks,
				max_swap_request_duration_blocks: swap_limits.max_swap_request_duration_blocks,
				minimum_chunk_size: any::AssetMap::try_from_fn(|asset| {
					api.cf_minimum_chunk_size(hash, asset).map(Into::into)
				})?,
			})
		})
	}

	fn cf_funding_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<FundingEnvironment> {
		self.with_runtime_api(at, |api, hash| {
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
		self.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(PoolsEnvironment {
				fees: {
					let mut map = AssetMap::default();
					for asset_pair in api.cf_pools(hash)? {
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
		self.new_subscription(
			Default::default(), /* notification_behaviour */
			true,               /* only_on_changes */
			false,              /* end_on_error */
			pending_sink,
			move |client, hash| {
				Ok((*client.runtime_api()).cf_pool_price(hash, from_asset, to_asset)?)
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
		self.new_subscription(
			Default::default(), /* notification_behaviour */
			false,              /* only_on_changes */
			true,               /* end_on_error */
			pending_sink,
			move |client, hash| {
				Ok(PoolPriceV2 {
					base_asset,
					quote_asset,
					price: (*client.runtime_api()).cf_pool_price_v2(
						hash,
						base_asset,
						quote_asset,
					)??,
				})
			},
		)
		.await
	}

	async fn cf_subscribe_transaction_screening_events(
		&self,
		pending_sink: PendingSubscriptionSink,
	) {
		self.new_subscription(
			NotificationBehaviour::Finalized, /* only_finalized */
			false,                            /* only_on_changes */
			true,                             /* end_on_error */
			pending_sink,
			move |client, hash| Ok((*client.runtime_api()).cf_transaction_screening_events(hash)?),
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
		let Ok(Ok(_)) = self.client.runtime_api().cf_pool_info(
			self.client.info().best_hash,
			base_asset,
			quote_asset,
		) else {
			pending_sink.reject(call_error("requested pool does not exist")).await;
			return;
		};

		self.new_subscription(
			Default::default(), /* notification_behaviour */
			false,              /* only_on_changes */
			true,               /* end_on_error */
			pending_sink,
			move |client, hash| {
				Ok(SwapResponse {
					swaps: (*client.runtime_api())
						.cf_scheduled_swaps(hash, base_asset, quote_asset)?
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
		let _ = (*self.client.runtime_api()).cf_pool_info(
			self.client.info().best_hash,
			base_asset,
			quote_asset,
		)?;

		self.with_runtime_api(at, |api, hash| api.cf_scheduled_swaps(hash, base_asset, quote_asset))
			.map(|swaps| {
				swaps
					.into_iter()
					.map(|(swap, execute_at)| ScheduledSwap::new(swap, execute_at))
					.collect()
			})
	}

	async fn cf_subscribe_prewitness_swaps(
		&self,
		pending_sink: PendingSubscriptionSink,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
	) {
		self.new_subscription(
			Default::default(), /* notification_behaviour */
			false,              /* only_on_changes */
			true,               /* end_on_error */
			pending_sink,
			move |client, hash| {
				Ok::<_, CfApiError>(RpcPrewitnessedSwap {
					base_asset,
					quote_asset,
					side,
					amounts: (*client.runtime_api())
						.cf_prewitness_swaps(hash, base_asset, quote_asset, side)?
						.into_iter()
						.map(|s| s.into())
						.collect(),
				})
			},
		)
		.await
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
				.cf_prewitness_swaps(self.unwrap_or_best(at), base_asset, quote_asset, side)?
				.into_iter()
				.map(|s| s.into())
				.collect(),
		})
	}

	async fn cf_subscribe_lp_order_fills(&self, sink: PendingSubscriptionSink) {
		self.new_subscription(
			NotificationBehaviour::Finalized,
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
			self.client.as_ref(),
			at.unwrap_or_else(|| self.client.info().finalized_hash),
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
			self.with_runtime_api(at, |api, hash| {
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
			self.with_runtime_api(at, |api, hash| {
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
		self.with_runtime_api(at, |api, hash| api.cf_pools(hash))
	}

	fn cf_solana_electoral_data(
		&self,
		validator: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.with_runtime_api(at, |api, hash| api.cf_electoral_data(hash, validator))
	}

	fn cf_solana_filter_votes(
		&self,
		validator: state_chain_runtime::AccountId,
		proposed_votes: Vec<u8>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<u8>> {
		self.with_runtime_api(at, |api, hash| api.cf_filter_votes(hash, validator, proposed_votes))
	}

	fn cf_get_vault_swap_details(
		&self,
		broker: state_chain_runtime::AccountId,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<state_chain_runtime::AccountId>>,
		dca_parameters: Option<DcaParameters>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<VaultSwapDetails<AddressString>> {
		self.with_runtime_api(at, |api, hash| {
			Ok::<_, CfApiError>(
				api.cf_get_vault_swap_details(
					hash,
					broker,
					source_asset,
					destination_asset,
					destination_address.try_parse_to_encoded_address(destination_asset.into())?,
					broker_commission,
					extra_parameters
						.try_into_encoded_params(source_asset.into())
						.map_err(DispatchErrorWithMessage::from)?,
					channel_metadata,
					boost_fee.unwrap_or_default(),
					affiliate_fees.unwrap_or_default(),
					dca_parameters,
				)??
				.map_btc_address(Into::into),
			)
		})
	}

	fn cf_get_transaction_screening_events(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<TransactionScreeningEvents> {
		self.with_runtime_api(at, |api, hash| api.cf_transaction_screening_events(hash))
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationBehaviour {
	/// Subscription will return finalized blocks.
	Finalized,
	/// Subscription will return best blocks. In the case of a re-org it might drop events.
	#[default]
	Best,
	/// Subscription will return all new blocks. In the case of a re-org it might duplicate events.
	///
	/// The caller is responsible for de-duplicating events.
	New,
}

impl<C, B, BE> CustomRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	B::Header: Unpin,
	BE: Send + Sync + 'static + Backend<B>,
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
	C::Api: CustomRuntimeApi<B>,
{
	fn chain_head_api(&self) -> RpcModule<ChainHead<BE, B, C>> {
		ChainHead::new(
			self.client.clone(),
			self.backend.clone(),
			self.executor.clone(),
			ChainHeadConfig::default(),
		)
		.into_rpc()
	}

	async fn new_subscription<
		T: Serialize + Send + Clone + Eq + 'static,
		F: Fn(&C, state_chain_runtime::Hash) -> Result<T, CfApiError> + Send + Clone + 'static,
	>(
		&self,
		notification_behaviour: NotificationBehaviour,
		only_on_changes: bool,
		end_on_error: bool,
		sink: PendingSubscriptionSink,
		f: F,
	) {
		self.new_subscription_with_state(
			notification_behaviour,
			only_on_changes,
			end_on_error,
			sink,
			move |client, hash, _state| f(client, hash).map(|res| (res, ())),
		)
		.await
	}

	/// The subscription will return the first value immediately and then either return new values
	/// only when it changes, or every new block.
	/// Note depending on the notification_behaviour blocks can be skipped. Also this
	/// subscription can either filter out, or end the stream if the provided async closure returns
	/// an error.
	async fn new_subscription_with_state<
		T: Serialize + Send + Clone + Eq + 'static,
		// State to carry forward between calls to the closure.
		S: 'static + Clone + Send,
		F: Fn(&C, state_chain_runtime::Hash, Option<&S>) -> Result<(T, S), CfApiError>
			+ Send
			+ Clone
			+ 'static,
	>(
		&self,
		notification_behaviour: NotificationBehaviour,
		only_on_changes: bool,
		end_on_error: bool,
		pending_sink: PendingSubscriptionSink,
		f: F,
	) {
		// subscribe to the chain head
		let Ok(subscription) =
			self.chain_head_api().subscribe_unbounded("chainHead_v1_follow", [false]).await
		else {
			pending_sink
				.reject(internal_error("chainHead_v1_follow subscription failed"))
				.await;
			return;
		};

		// construct either best, new or finalized blocks stream from the chain head subscription
		let blocks_stream = stream::unfold(subscription, move |mut sub| async move {
			match sub.next::<FollowEvent<Hash>>().await {
				Some(Ok((event, _subs_id))) => Some((event, sub)),
				Some(Err(e)) => {
					log::warn!("ChainHead subscription error {:?}", e);
					None
				},
				_ => None,
			}
		})
		.filter_map(move |event| async move {
			// When NotificationBehaviour is:
			// * NotificationBehaviour::Finalized: listen to initialized and finalized events
			// * NotificationBehaviour::Best: listen to just bestBlockChanged events
			// * NotificationBehaviour::New: listen to just newBlock events
			// See: https://paritytech.github.io/json-rpc-interface-spec/api/chainHead_v1_follow.html
			match (notification_behaviour, event) {
				(
					// Always start from the most recent finalized block hash
					NotificationBehaviour::Finalized,
					FollowEvent::Initialized(sc_rpc_spec_v2::chain_head::Initialized {
						mut finalized_block_hashes,
						..
					}),
				) => Some(vec![finalized_block_hashes
					.pop()
					.expect("Guaranteed to have at least one element.")]),
				(
					NotificationBehaviour::Finalized,
					FollowEvent::Finalized(sc_rpc_spec_v2::chain_head::Finalized {
						finalized_block_hashes,
						..
					}),
				) => Some(finalized_block_hashes),
				(
					NotificationBehaviour::Best,
					FollowEvent::BestBlockChanged(sc_rpc_spec_v2::chain_head::BestBlockChanged {
						best_block_hash,
					}),
				) => Some(vec![best_block_hash]),
				(
					NotificationBehaviour::New,
					FollowEvent::NewBlock(sc_rpc_spec_v2::chain_head::NewBlock {
						block_hash, ..
					}),
				) => Some(vec![block_hash]),
				_ => None,
			}
		})
		.map(stream::iter)
		.flatten();

		let stream = blocks_stream
			.filter_map({
				let client = self.client.clone();

				let mut previous_item = None;
				let mut previous_state = None;

				move |hash| {
					futures::future::ready(match f(&client, hash, previous_state.as_ref()) {
						Ok((new_item, new_state))
							if !only_on_changes || Some(&new_item) != previous_item.as_ref() =>
						{
							previous_item = Some(new_item.clone());
							previous_state = Some(new_state);

							if let Ok(Some(header)) = client.header(hash) {
								Some(Ok(BlockUpdate {
									block_hash: hash,
									block_number: *header.number(),
									data: new_item,
								}))
							} else if end_on_error {
								Some(Err(internal_error(format!(
									"Could not fetch block header for block {:?}",
									hash
								))))
							} else {
								None
							}
						},
						Err(error) => {
							log::warn!("Subscription Error: {error}.");
							if end_on_error {
								log::warn!("Closing Subscription.");
								Some(Err(ErrorObjectOwned::from(error)))
							} else {
								None
							}
						},
						_ => None,
					})
				}
			})
			.take_while(|item| futures::future::ready(item.is_ok()))
			.map(Result::unwrap)
			.boxed();

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
	use cf_chains::{assets::sol, btc::ScriptPubkey};
	use cf_primitives::{
		chains::assets::{any, arb, btc, dot, eth, hub},
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
		use cf_chains::btc::BitcoinNetwork;
		let broker = RpcAccountInfo::broker(
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
				],
				btc_vault_deposit_address: Some(
					ScriptPubkey::Taproot([1u8; 32]).to_address(&BitcoinNetwork::Testnet),
				),
				affiliates: vec![(cf_primitives::AffiliateShortId(1), AccountId32::new([1; 32]))],
				bond: 0,
			},
			0,
		);
		insta::assert_snapshot!(serde_json::to_value(broker).unwrap());
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
					hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
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
					hub: hub::AssetMap { dot: None, usdc: None, usdt: None },
				},
				network_fee_hundredth_pips: Permill::from_percent(100),
				swap_retry_delay_blocks: 5,
				max_swap_retry_duration_blocks: 600,
				max_swap_request_duration_blocks: 14400,
				minimum_chunk_size: any::AssetMap {
					eth: eth::AssetMap {
						eth: 123_u32.into(),
						flip: 0u32.into(),
						usdc: 456_u32.into(),
						usdt: 0u32.into(),
					},
					btc: btc::AssetMap { btc: 789_u32.into() },
					dot: dot::AssetMap { dot: 0u32.into() },
					arb: arb::AssetMap { eth: 0u32.into(), usdc: 101112_u32.into() },
					sol: sol::AssetMap { sol: 0u32.into(), usdc: 0u32.into() },
					hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
				},
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
					hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
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
					hub: hub::AssetMap {
						dot: Some((u64::MAX / 2 - 1).into()),
						usdc: None,
						usdt: None,
					},
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
					hub: hub::AssetMap {
						dot: Some((u64::MAX / 2 - 1).into()),
						usdc: None,
						usdt: None,
					},
				},
				witness_safety_margins: HashMap::from([
					(ForeignChain::Bitcoin, Some(3u64)),
					(ForeignChain::Ethereum, Some(3u64)),
					(ForeignChain::Polkadot, None),
					(ForeignChain::Arbitrum, None),
					(ForeignChain::Solana, None),
					(ForeignChain::Assethub, None),
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
					hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
				},
				channel_opening_fees: HashMap::from([
					(ForeignChain::Bitcoin, 0u32.into()),
					(ForeignChain::Ethereum, 1000u32.into()),
					(ForeignChain::Polkadot, 1000u32.into()),
					(ForeignChain::Arbitrum, 1000u32.into()),
					(ForeignChain::Solana, 1000u32.into()),
					(ForeignChain::Assethub, 1000u32.into()),
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
						hub: hub::AssetMap {
							dot: Some(pool_info),
							usdc: Some(pool_info),
							usdt: Some(pool_info),
						},
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
			network_fee_deduction_percent: Percent::from_percent(40),
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
			network_fee_deduction_percent: Percent::from_percent(0),
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
			broker_commission: RpcFee { asset: Asset::Usdc, amount: 100u128.into() },
		})
		.unwrap());
	}
}
