use cf_amm::{
	common::{Amount, Order, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{ForeignChainAddressHumanreadable, ToHumanreadableAddress},
	eth::Address as EthereumAddress,
	Chain,
};
use cf_primitives::{
	AccountRole, Asset, AssetAmount, BroadcastId, ForeignChain, NetworkEnvironment, SemVer,
	SwapOutput,
};
use cf_utilities::rpc::NumberOrHex;
use core::ops::Range;
use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{CallError, SubscriptionEmptyError},
	SubscriptionSink,
};
use pallet_cf_governance::GovCallHash;
use pallet_cf_pools::{
	AskBidMap, AssetsMap, PoolInfo, PoolLiquidity, PoolPriceV1, UnidirectionalPoolDepth,
};
use sc_client_api::{BlockchainEvents, HeaderBackend};
use serde::{Deserialize, Serialize};
use sp_api::{BlockT, HeaderT};
use sp_core::U256;
use sp_runtime::Permill;
use state_chain_runtime::{
	chainflip::{BlockUpdate, Offence},
	constants::common::TX_FEE_MULTIPLIER,
	runtime_apis::{
		CustomRuntimeApi, DispatchErrorWithMessage, FailingWitnessValidators,
		LiquidityProviderInfo, RuntimeApiAccountInfoV2,
	},
	NetworkFee,
};
use std::{
	collections::{BTreeMap, HashMap},
	marker::PhantomData,
	sync::Arc,
};

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
#[serde(
	expecting = r#"Expected a valid asset specifier. Assets should be specified as upper-case strings, e.g. `"ETH"`, and can be optionally distinguished by chain, e.g. `{ chain: "Ethereum", asset: "ETH" }."#
)]
pub enum RpcAsset {
	ImplicitChain(Asset),
	ExplicitChain { chain: ForeignChain, asset: Asset },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RpcAssetWithAmount {
	#[serde(flatten)]
	pub asset: RpcAsset,
	pub amount: AssetAmount,
}

impl TryInto<Asset> for RpcAsset {
	type Error = AssetConversionError;

	fn try_into(self) -> Result<Asset, Self::Error> {
		match self {
			RpcAsset::ImplicitChain(asset) => Ok(asset),
			RpcAsset::ExplicitChain { chain, asset } =>
				if chain == ForeignChain::from(asset) {
					Ok(asset)
				} else {
					Err(AssetConversionError::UnsupportedAsset(chain, asset))
				},
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AssetConversionError {
	#[error("Unsupported asset {1:?} on chain {0}")]
	UnsupportedAsset(ForeignChain, Asset),
}

impl From<AssetConversionError> for jsonrpsee::core::Error {
	fn from(e: AssetConversionError) -> Self {
		CallError::from_std_error(e).into()
	}
}

impl TryFrom<(Asset, Option<ForeignChain>)> for RpcAsset {
	type Error = AssetConversionError;

	fn try_from((asset, chain): (Asset, Option<ForeignChain>)) -> Result<Self, Self::Error> {
		match chain {
			None => Ok(RpcAsset::ExplicitChain { asset, chain: asset.into() }),
			Some(chain) =>
				if chain == ForeignChain::from(asset) {
					Ok(RpcAsset::ExplicitChain { asset, chain })
				} else {
					Err(AssetConversionError::UnsupportedAsset(chain, asset))
				},
		}
	}
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum RpcAccountInfo {
	Unregistered {
		flip_balance: NumberOrHex,
	},
	Broker {
		flip_balance: NumberOrHex,
	},
	LiquidityProvider {
		balances: HashMap<ForeignChain, HashMap<Asset, NumberOrHex>>,
		refund_addresses: HashMap<ForeignChain, Option<ForeignChainAddressHumanreadable>>,
		flip_balance: NumberOrHex,
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

	fn broker(balance: u128) -> Self {
		Self::Broker { flip_balance: balance.into() }
	}

	fn lp(info: LiquidityProviderInfo, network: NetworkEnvironment, balance: u128) -> Self {
		let mut balances = HashMap::new();

		for (asset, balance) in info.balances {
			balances
				.entry(asset.into())
				.or_insert_with(HashMap::new)
				.insert(asset, balance.into());
		}

		Self::LiquidityProvider {
			flip_balance: balance.into(),
			balances,
			refund_addresses: info
				.refund_addresses
				.into_iter()
				.map(|(chain, address)| (chain, address.map(|a| a.to_humanreadable(network))))
				.collect(),
		}
	}

	fn validator(info: RuntimeApiAccountInfoV2) -> Self {
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

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub struct RpcPenalty {
	reputation_points: i32,
	suspension_duration_blocks: u32,
}

type RpcSuspensions = Vec<(Offence, Vec<(u32, state_chain_runtime::AccountId)>)>;

#[derive(Serialize, Deserialize)]
pub struct RpcAuctionState {
	blocks_per_epoch: u32,
	current_epoch_started_at: u32,
	redemption_period_as_percentage: u8,
	min_funding: NumberOrHex,
	auction_size_range: (u32, u32),
	min_active_bid: Option<NumberOrHex>,
}

#[derive(Serialize, Deserialize)]
pub struct RpcSwapOutput {
	// Intermediary amount, if there's any
	pub intermediary: Option<NumberOrHex>,
	// Final output of the swap
	pub output: NumberOrHex,
}

impl From<SwapOutput> for RpcSwapOutput {
	fn from(swap_output: SwapOutput) -> Self {
		Self {
			intermediary: swap_output.intermediary.map(Into::into),
			output: swap_output.output.into(),
		}
	}
}

impl From<Asset> for RpcAsset {
	fn from(asset: Asset) -> Self {
		RpcAsset::ExplicitChain { asset, chain: asset.into() }
	}
}

#[derive(Serialize, Deserialize)]
pub struct RpcPoolInfo {
	#[serde(flatten)]
	pub pool_info: PoolInfo,
	pub quote_asset: RpcAsset,
}

impl From<PoolInfo> for RpcPoolInfo {
	fn from(pool_info: PoolInfo) -> Self {
		Self { pool_info, quote_asset: Asset::Usdc.into() }
	}
}

#[derive(Serialize, Deserialize)]
pub struct PoolsEnvironment {
	pub fees: HashMap<ForeignChain, HashMap<Asset, Option<RpcPoolInfo>>>,
}

#[derive(Serialize, Deserialize)]
pub struct IngressEgressEnvironment {
	pub minimum_deposit_amounts: HashMap<ForeignChain, HashMap<Asset, NumberOrHex>>,
	pub ingress_fees: HashMap<ForeignChain, HashMap<Asset, Option<NumberOrHex>>>,
	pub egress_fees: HashMap<ForeignChain, HashMap<Asset, Option<NumberOrHex>>>,
	pub witness_safety_margins: HashMap<ForeignChain, Option<u64>>,
	pub egress_dust_limits: HashMap<ForeignChain, HashMap<Asset, NumberOrHex>>,
}

#[derive(Serialize, Deserialize)]
pub struct FundingEnvironment {
	pub redemption_tax: NumberOrHex,
	pub minimum_funding_amount: NumberOrHex,
}

#[derive(Serialize, Deserialize)]
pub struct SwappingEnvironment {
	maximum_swap_amounts: HashMap<ForeignChain, HashMap<Asset, Option<NumberOrHex>>>,
	network_fee_hundredth_pips: Permill,
}

#[derive(Serialize, Deserialize)]
pub struct RpcEnvironment {
	ingress_egress: IngressEgressEnvironment,
	swapping: SwappingEnvironment,
	funding: FundingEnvironment,
	pools: PoolsEnvironment,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PoolPriceV2 {
	pub base_asset: RpcAsset,
	pub quote_asset: RpcAsset,
	#[serde(flatten)]
	pub price: pallet_cf_pools::PoolPriceV2,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RpcPrewitnessedSwap {
	pub base_asset: RpcAsset,
	pub quote_asset: RpcAsset,
	pub side: Order,
	pub amounts: Vec<U256>,
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
	) -> RpcResult<RpcAccountInfo>;
	#[method(name = "account_info_v2")]
	fn cf_account_info_v2(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfoV2>;
	#[method(name = "asset_balances")]
	fn cf_asset_balances(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcAssetWithAmount>>;
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
		from_asset: RpcAsset,
		to_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolPriceV1>>;
	#[method(name = "pool_price_v2")]
	fn cf_pool_price_v2(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPriceV2>;
	#[method(name = "swap_rate")]
	fn cf_pool_swap_rate(
		&self,
		from_asset: RpcAsset,
		to_asset: RpcAsset,
		amount: NumberOrHex,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutput>;
	#[method(name = "required_asset_ratio_for_range_order")]
	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AssetsMap<Amount>>;
	#[method(name = "pool_orderbook")]
	fn cf_pool_orderbook(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		orders: u32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrderbook>;
	#[method(name = "pool_info")]
	fn cf_pool_info(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolInfo>;
	#[method(name = "pool_depth")]
	fn cf_pool_depth(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AskBidMap<UnidirectionalPoolDepth>>;
	#[method(name = "pool_liquidity")]
	fn cf_pool_liquidity(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolLiquidity>;
	#[method(name = "pool_orders")]
	fn cf_pool_orders(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		lp: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrders<state_chain_runtime::Runtime>>;
	#[method(name = "pool_range_order_liquidity_value")]
	fn cf_pool_range_order_liquidity_value(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AssetsMap<Amount>>;
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
	#[method(name = "environment")]
	fn cf_environment(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEnvironment>;
	#[deprecated(note = "Use direct storage access of `CurrentReleaseVersion` instead.")]
	#[method(name = "current_compatibility_version")]
	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer>;

	#[method(name = "max_swap_amount")]
	fn cf_max_swap_amount(&self, asset: RpcAsset) -> RpcResult<Option<AssetAmount>>;
	#[subscription(name = "subscribe_pool_price", item = PoolPriceV1)]
	fn cf_subscribe_pool_price(&self, from_asset: RpcAsset, to_asset: RpcAsset);
	#[subscription(name = "subscribe_pool_price_v2", item = BlockUpdate<PoolPriceV2>)]
	fn cf_subscribe_pool_price_v2(&self, base_asset: RpcAsset, quote_asset: RpcAsset);
	#[subscription(name = "subscribe_prewitness_swaps", item = BlockUpdate<RpcPrewitnessedSwap>)]
	fn cf_subscribe_prewitness_swaps(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
	);

	#[method(name = "prewitness_swaps")]
	fn cf_prewitness_swaps(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcPrewitnessedSwap>;

	#[method(name = "supported_assets")]
	fn cf_supported_assets(&self) -> RpcResult<HashMap<ForeignChain, Vec<Asset>>>;

	#[method(name = "failed_call")]
	fn cf_failed_call(
		&self,
		broadcast_id: BroadcastId,
	) -> RpcResult<Option<<cf_chains::Ethereum as Chain>::Transaction>>;

	#[method(name = "witness_count")]
	fn cf_witness_count(
		&self,
		hash: state_chain_runtime::Hash,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<FailingWitnessValidators>>;
}

/// An RPC extension for the state chain node.
pub struct CustomRpc<C, B> {
	pub client: Arc<C>,
	pub _phantom: PhantomData<B>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
}

impl<C, B> CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ HeaderBackend<B>
		+ BlockchainEvents<B>,
	C::Api: CustomRuntimeApi<B>,
{
	fn unwrap_or_best(&self, from_rpc: Option<<B as BlockT>::Hash>) -> B::Hash {
		from_rpc.unwrap_or_else(|| self.client.info().best_hash)
	}
}

fn to_rpc_error<E: std::error::Error + Send + Sync + 'static>(e: E) -> jsonrpsee::core::Error {
	CallError::from_std_error(e).into()
}

fn map_dispatch_error(e: DispatchErrorWithMessage) -> jsonrpsee::core::Error {
	jsonrpsee::core::Error::from(match e {
		DispatchErrorWithMessage::Module(message) => match std::str::from_utf8(&message) {
			Ok(message) => anyhow::anyhow!("DispatchError: {message}"),
			Err(error) =>
				anyhow::anyhow!("DispatchError: Unable to deserialize error message: '{error}'"),
		},
		DispatchErrorWithMessage::Other(e) =>
			anyhow::anyhow!("DispatchError: {}", <&'static str>::from(e)),
	})
}

impl<C, B> CustomApiServer for CustomRpc<C, B>
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
				AccountRole::Broker => RpcAccountInfo::broker(balance),
				AccountRole::LiquidityProvider => {
					let info = api
						.cf_liquidity_provider_info(hash, account_id)
						.map_err(to_rpc_error)?
						.expect("role already validated");

					RpcAccountInfo::lp(
						info,
						api.cf_network_environment(hash).map_err(to_rpc_error)?,
						balance,
					)
				},
				AccountRole::Validator => {
					let info = api.cf_account_info_v2(hash, &account_id).map_err(to_rpc_error)?;

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
			.cf_account_info_v2(self.unwrap_or_best(at), &account_id)
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

	fn cf_asset_balances(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcAssetWithAmount>> {
		Ok(self
			.client
			.runtime_api()
			.cf_asset_balances(self.unwrap_or_best(at), account_id)
			.map_err(to_rpc_error)?
			.into_iter()
			.map(|(asset, balance)| RpcAssetWithAmount { asset: asset.into(), amount: balance })
			.collect::<Vec<RpcAssetWithAmount>>())
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
		from_asset: RpcAsset,
		to_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolPriceV1>> {
		self.client
			.runtime_api()
			.cf_pool_price(self.unwrap_or_best(at), from_asset.try_into()?, to_asset.try_into()?)
			.map_err(to_rpc_error)
	}

	fn cf_pool_price_v2(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolPriceV2> {
		let base = base_asset.try_into()?;
		let quote = quote_asset.try_into()?;
		let hash = self.unwrap_or_best(at);
		Ok(PoolPriceV2 {
			base_asset,
			quote_asset,
			price: self
				.client
				.runtime_api()
				.cf_pool_price_v2(hash, base, quote)
				.map_err(to_rpc_error)
				.and_then(|result| result.map_err(map_dispatch_error))?,
		})
	}

	fn cf_pool_swap_rate(
		&self,
		from_asset: RpcAsset,
		to_asset: RpcAsset,
		amount: NumberOrHex,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutput> {
		self.client
			.runtime_api()
			.cf_pool_simulate_swap(
				self.unwrap_or_best(at),
				from_asset.try_into()?,
				to_asset.try_into()?,
				amount
					.try_into()
					.and_then(|amount| {
						if amount == 0 {
							Err("Swap input amount cannot be zero.")
						} else {
							Ok(amount)
						}
					})
					.map_err(|str| anyhow::anyhow!(str))?,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
			.map(RpcSwapOutput::from)
	}

	fn cf_pool_info(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolInfo> {
		self.client
			.runtime_api()
			.cf_pool_info(self.unwrap_or_best(at), base_asset.try_into()?, quote_asset.try_into()?)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_depth(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		tick_range: Range<Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AskBidMap<UnidirectionalPoolDepth>> {
		self.client
			.runtime_api()
			.cf_pool_depth(
				self.unwrap_or_best(at),
				base_asset.try_into()?,
				quote_asset.try_into()?,
				tick_range,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_liquidity(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PoolLiquidity> {
		self.client
			.runtime_api()
			.cf_pool_liquidity(
				self.unwrap_or_best(at),
				base_asset.try_into()?,
				quote_asset.try_into()?,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AssetsMap<Amount>> {
		self.client
			.runtime_api()
			.cf_required_asset_ratio_for_range_order(
				self.unwrap_or_best(at),
				base_asset.try_into()?,
				quote_asset.try_into()?,
				tick_range,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_orderbook(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		orders: u32,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrderbook> {
		self.client
			.runtime_api()
			.cf_pool_orderbook(
				self.unwrap_or_best(at),
				base_asset.try_into()?,
				quote_asset.try_into()?,
				orders,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map(Into::into).map_err(map_dispatch_error))
	}

	fn cf_pool_orders(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		lp: Option<state_chain_runtime::AccountId>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<pallet_cf_pools::PoolOrders<state_chain_runtime::Runtime>> {
		self.client
			.runtime_api()
			.cf_pool_orders(
				self.unwrap_or_best(at),
				base_asset.try_into()?,
				quote_asset.try_into()?,
				lp,
			)
			.map_err(to_rpc_error)
			.and_then(|result| result.map_err(map_dispatch_error))
	}

	fn cf_pool_range_order_liquidity_value(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AssetsMap<Amount>> {
		self.client
			.runtime_api()
			.cf_pool_range_order_liquidity_value(
				self.unwrap_or_best(at),
				base_asset.try_into()?,
				quote_asset.try_into()?,
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
		let mut minimum_deposit_amounts = HashMap::new();
		let mut ingress_fees = HashMap::new();
		let mut egress_fees = HashMap::new();
		let mut witness_safety_margins = HashMap::new();
		let mut egress_dust_limits = HashMap::new();

		for asset in Asset::all() {
			let chain = ForeignChain::from(asset);
			minimum_deposit_amounts.entry(chain).or_insert_with(HashMap::new).insert(
				asset,
				runtime_api.cf_min_deposit_amount(hash, asset).map_err(to_rpc_error)?.into(),
			);
			ingress_fees.entry(chain).or_insert_with(HashMap::new).insert(
				asset,
				runtime_api.cf_ingress_fee(hash, asset).map_err(to_rpc_error)?.map(Into::into),
			);
			egress_fees.entry(chain).or_insert_with(HashMap::new).insert(
				asset,
				runtime_api.cf_egress_fee(hash, asset).map_err(to_rpc_error)?.map(Into::into),
			);
			egress_dust_limits.entry(chain).or_insert_with(HashMap::new).insert(
				asset,
				runtime_api.cf_egress_dust_limit(hash, asset).map_err(to_rpc_error)?.into(),
			);
		}

		for chain in ForeignChain::iter() {
			witness_safety_margins.insert(
				chain,
				runtime_api.cf_witness_safety_margin(hash, chain).map_err(to_rpc_error)?,
			);
		}

		Ok(IngressEgressEnvironment {
			minimum_deposit_amounts,
			ingress_fees,
			egress_fees,
			witness_safety_margins,
			egress_dust_limits,
		})
	}

	fn cf_swapping_environment(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<SwappingEnvironment> {
		let runtime_api = &self.client.runtime_api();
		let hash = self.unwrap_or_best(at);

		let mut maximum_swap_amounts = HashMap::new();

		for asset in Asset::all() {
			let max_amount = runtime_api.cf_max_swap_amount(hash, asset).map_err(to_rpc_error)?;
			maximum_swap_amounts
				.entry(asset.into())
				.or_insert_with(HashMap::new)
				.insert(asset, max_amount.map(|amt| amt.into()));
		}

		Ok(SwappingEnvironment {
			maximum_swap_amounts,
			network_fee_hundredth_pips: NetworkFee::get(),
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
		let mut fees = HashMap::new();

		for asset in Asset::all() {
			if asset == Asset::Usdc {
				continue
			}

			let info = self.cf_pool_info(asset.into(), Asset::Usdc.into(), at).ok().map(Into::into);

			fees.entry(asset.into()).or_insert_with(HashMap::new).insert(asset, info);
		}

		Ok(PoolsEnvironment { fees })
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

	fn cf_max_swap_amount(&self, asset: RpcAsset) -> RpcResult<Option<AssetAmount>> {
		self.client
			.runtime_api()
			.cf_max_swap_amount(self.unwrap_or_best(None), asset.try_into()?)
			.map_err(to_rpc_error)
	}

	fn cf_subscribe_pool_price(
		&self,
		sink: SubscriptionSink,
		from_asset: RpcAsset,
		to_asset: RpcAsset,
	) -> Result<(), SubscriptionEmptyError> {
		let from = from_asset.try_into().map_err(|_| SubscriptionEmptyError)?;
		let to = to_asset.try_into().map_err(|_| SubscriptionEmptyError)?;
		self.new_subscription(
			true,  /* only_on_changes */
			false, /* end_on_error */
			sink,
			move |api, hash| api.cf_pool_price(hash, from, to),
		)
	}

	fn cf_subscribe_pool_price_v2(
		&self,
		sink: SubscriptionSink,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
	) -> Result<(), SubscriptionEmptyError> {
		let base_asset_inner = base_asset.try_into().map_err(|_| SubscriptionEmptyError)?;
		let quote_asset_inner = quote_asset.try_into().map_err(|_| SubscriptionEmptyError)?;
		self.new_subscription(
			false, /* only_on_changes */
			true,  /* end_on_error */
			sink,
			move |api, hash| {
				api.cf_pool_price_v2(hash, base_asset_inner, quote_asset_inner)
					.map_err(to_rpc_error)
					.and_then(|result| result.map_err(map_dispatch_error))
					.map(|price| PoolPriceV2 { base_asset, quote_asset, price })
			},
		)
	}

	fn cf_subscribe_prewitness_swaps(
		&self,
		sink: SubscriptionSink,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
	) -> Result<(), SubscriptionEmptyError> {
		let base_asset_inner = base_asset.try_into().map_err(|_| SubscriptionEmptyError)?;
		let quote_asset_inner = quote_asset.try_into().map_err(|_| SubscriptionEmptyError)?;
		self.new_subscription(
			false, /* only_on_changes */
			true,  /* end_on_error */
			sink,
			move |api, hash| {
				Ok::<RpcPrewitnessedSwap, jsonrpsee::core::Error>(RpcPrewitnessedSwap {
					base_asset,
					quote_asset,
					side,
					amounts: api
						.cf_prewitness_swaps(hash, base_asset_inner, quote_asset_inner, side)
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
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcPrewitnessedSwap> {
		Ok(RpcPrewitnessedSwap {
			base_asset,
			quote_asset,
			side,
			amounts: self
				.client
				.runtime_api()
				.cf_prewitness_swaps(
					self.unwrap_or_best(at),
					base_asset.try_into()?,
					quote_asset.try_into()?,
					side,
				)
				.map_err(to_rpc_error)?
				.into_iter()
				.map(|s| s.into())
				.collect(),
		})
	}

	fn cf_supported_assets(&self) -> RpcResult<HashMap<ForeignChain, Vec<Asset>>> {
		let mut chain_to_asset: HashMap<ForeignChain, Vec<Asset>> = HashMap::new();
		Asset::all().iter().for_each(|asset| {
			chain_to_asset
				.entry((*asset).into())
				.and_modify(|asset_vec| asset_vec.push(*asset))
				.or_insert(vec![*asset]);
		});
		Ok(chain_to_asset)
	}

	fn cf_failed_call(
		&self,
		broadcast_id: BroadcastId,
	) -> RpcResult<Option<<cf_chains::Ethereum as Chain>::Transaction>> {
		self.client
			.runtime_api()
			.cf_failed_call(self.unwrap_or_best(None), broadcast_id)
			.map_err(to_rpc_error)
	}

	fn cf_witness_count(
		&self,
		hash: state_chain_runtime::Hash,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<FailingWitnessValidators>> {
		self.client
			.runtime_api()
			.cf_witness_count(self.unwrap_or_best(at), pallet_cf_witnesser::CallHash(hash.into()))
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
	/// The subscription will return the first value immediately and then either return new values
	/// only when it changes, or every new block. Note in both cases this can skip blocks. Also this
	/// subscription can either filter out, or end the stream if the provided async closure returns
	/// an error.
	fn new_subscription<
		T: Serialize + Send + Clone + Eq + 'static,
		E: std::error::Error + Send + Sync + 'static,
		F: Fn(&C::Api, state_chain_runtime::Hash) -> Result<T, E> + Send + Clone + 'static,
	>(
		&self,
		only_on_changes: bool,
		end_on_error: bool,
		mut sink: SubscriptionSink,
		f: F,
	) -> Result<(), SubscriptionEmptyError> {
		use futures::{future::FutureExt, stream::StreamExt};

		let info = self.client.info();

		let initial = match f(&self.client.runtime_api(), info.best_hash) {
			Ok(initial) => initial,
			Err(e) => {
				let _ = sink.reject(jsonrpsee::core::Error::from(
					sc_rpc_api::state::error::Error::Client(Box::new(e)),
				));
				return Ok(())
			},
		};
		let stream = futures::stream::iter(std::iter::once(Ok(BlockUpdate {
			block_hash: info.best_hash,
			block_number: info.best_number,
			data: initial.clone(),
		})))
		.chain(
			self.client
				.import_notification_stream()
				.filter(|n| futures::future::ready(n.is_new_best))
				.filter_map({
					let client = self.client.clone();
					let mut previous = initial;
					move |n| {
						futures::future::ready(match f(&client.runtime_api(), n.hash) {
							Ok(new) if !only_on_changes || new != previous => {
								previous = new.clone();
								Some(Ok(BlockUpdate {
									block_hash: n.hash,
									block_number: *n.header.number(),
									data: new,
								}))
							},
							Err(error) if end_on_error => Some(Err(error)),
							_ => None,
						})
					}
				}),
		);

		self.executor.spawn(
			"cf-rpc-update-subscription",
			Some("rpc"),
			async move {
				sink.pipe_from_try_stream(stream).await;
			}
			.boxed(),
		);

		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use cf_primitives::FLIPPERINOS_PER_FLIP;
	use sp_core::H160;

	/*
		changing any of these serialization tests signifies a breaking change in the
		API. please make sure to get approval from the product team before merging
		any changes that break a serialization test.

		if approval is received and a new breaking change is introduced, please
		stale the review and get a new review from someone on product.
	*/

	#[test]
	fn test_no_account_serialization() {
		insta::assert_display_snapshot!(
			serde_json::to_value(RpcAccountInfo::unregistered(0)).unwrap()
		);
	}

	#[test]
	fn test_broker_serialization() {
		insta::assert_display_snapshot!(serde_json::to_value(RpcAccountInfo::broker(0)).unwrap());
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
				],
				balances: vec![
					(Asset::Eth, u128::MAX),
					(Asset::Btc, 0),
					(Asset::Flip, u128::MAX / 2),
				],
			},
			cf_primitives::NetworkEnvironment::Mainnet,
			0,
		);

		insta::assert_display_snapshot!(serde_json::to_value(lp).unwrap());
	}

	#[test]
	fn test_validator_serialization() {
		let validator = RpcAccountInfo::validator(RuntimeApiAccountInfoV2 {
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

		insta::assert_display_snapshot!(serde_json::to_value(validator).unwrap());
	}

	#[test]
	fn test_environment_serialization() {
		let env = RpcEnvironment {
			swapping: SwappingEnvironment {
				maximum_swap_amounts: HashMap::from([
					(ForeignChain::Bitcoin, HashMap::from([(Asset::Btc, Some(0u32.into()))])),
					(
						ForeignChain::Ethereum,
						HashMap::from([
							(Asset::Flip, None),
							(Asset::Usdc, Some((u64::MAX / 2 - 1).into())),
							(Asset::Eth, Some(0u32.into())),
						]),
					),
				]),
				network_fee_hundredth_pips: Permill::from_percent(100),
			},
			ingress_egress: IngressEgressEnvironment {
				minimum_deposit_amounts: HashMap::from([
					(ForeignChain::Bitcoin, HashMap::from([(Asset::Btc, 0u32.into())])),
					(
						ForeignChain::Ethereum,
						HashMap::from([
							(Asset::Flip, u64::MAX.into()),
							(Asset::Usdc, (u64::MAX / 2 - 1).into()),
							(Asset::Eth, 0u32.into()),
						]),
					),
				]),
				ingress_fees: HashMap::from([
					(
						ForeignChain::Bitcoin,
						HashMap::from([(Asset::Btc, Some(0u32).map(Into::into))]),
					),
					(
						ForeignChain::Ethereum,
						HashMap::<_, Option<NumberOrHex>>::from([
							(Asset::Flip, Some(AssetAmount::MAX).map(Into::into)),
							(Asset::Usdc, None),
							(Asset::Eth, Some(0u32).map(Into::into)),
						]),
					),
					(
						ForeignChain::Polkadot,
						HashMap::from([(Asset::Dot, Some(u64::MAX / 2 - 1).map(Into::into))]),
					),
				]),
				egress_fees: HashMap::from([
					(
						ForeignChain::Bitcoin,
						HashMap::from([(Asset::Btc, Some(0u32).map(Into::into))]),
					),
					(
						ForeignChain::Ethereum,
						HashMap::<_, Option<NumberOrHex>>::from([
							(Asset::Flip, Some(AssetAmount::MAX).map(Into::into)),
							(Asset::Usdc, None),
							(Asset::Eth, Some(0u32).map(Into::into)),
						]),
					),
					(
						ForeignChain::Polkadot,
						HashMap::from([(Asset::Dot, Some(u64::MAX / 2 - 1).map(Into::into))]),
					),
				]),
				witness_safety_margins: HashMap::from([
					(ForeignChain::Bitcoin, Some(3u64)),
					(ForeignChain::Ethereum, Some(3u64)),
					(ForeignChain::Polkadot, None),
				]),
				egress_dust_limits: HashMap::from([
					(ForeignChain::Bitcoin, HashMap::from([(Asset::Btc, 0u32.into())])),
					(
						ForeignChain::Ethereum,
						HashMap::from([
							(Asset::Flip, AssetAmount::MAX.into()),
							(Asset::Usdc, (u64::MAX / 2 - 1).into()),
							(Asset::Eth, 0u32.into()),
						]),
					),
				]),
			},
			funding: FundingEnvironment {
				redemption_tax: 0u32.into(),
				minimum_funding_amount: 0u32.into(),
			},
			pools: PoolsEnvironment {
				fees: HashMap::from([(
					ForeignChain::Ethereum,
					HashMap::from([(
						Asset::Flip,
						Some(
							PoolInfo {
								limit_order_fee_hundredth_pips: 0,
								range_order_fee_hundredth_pips: 100,
							}
							.into(),
						),
					)]),
				)]),
			},
		};

		insta::assert_display_snapshot!(serde_json::to_value(env).unwrap());
	}

	#[test]
	fn test_rpc_asset_foreign_chain_support() {
		fn try_into_asset(
			asset: Asset,
			chain: ForeignChain,
		) -> Result<Asset, AssetConversionError> {
			RpcAsset::ExplicitChain { asset, chain }.try_into()
		}

		// Test supported combinations
		assert_eq!(try_into_asset(Asset::Eth, ForeignChain::Ethereum).unwrap(), Asset::Eth);
		assert_eq!(try_into_asset(Asset::Flip, ForeignChain::Ethereum).unwrap(), Asset::Flip);
		assert_eq!(try_into_asset(Asset::Usdc, ForeignChain::Ethereum).unwrap(), Asset::Usdc);
		assert_eq!(try_into_asset(Asset::Dot, ForeignChain::Polkadot).unwrap(), Asset::Dot);
		assert_eq!(try_into_asset(Asset::Btc, ForeignChain::Bitcoin).unwrap(), Asset::Btc);
		let implicit_chain_asset: Asset = RpcAsset::ImplicitChain(Asset::Flip).try_into().unwrap();
		assert_eq!(implicit_chain_asset, Asset::Flip);

		// Test some unsupported combinations
		assert!(try_into_asset(Asset::Eth, ForeignChain::Polkadot).is_err());
		assert!(try_into_asset(Asset::Flip, ForeignChain::Polkadot).is_err());
		assert!(try_into_asset(Asset::Dot, ForeignChain::Ethereum).is_err());
		assert!(try_into_asset(Asset::Usdc, ForeignChain::Bitcoin).is_err());
		assert!(try_into_asset(Asset::Btc, ForeignChain::Ethereum).is_err());
	}

	#[test]
	fn test_failed_parse_error_message() {
		let error = serde_json::from_str::<RpcAsset>("\"Eth\"").unwrap_err();
		assert_eq!(
			error.to_string(),
			r#"Expected a valid asset specifier. Assets should be specified as upper-case strings, e.g. `"ETH"`, and can be optionally distinguished by chain, e.g. `{ chain: "Ethereum", asset: "ETH" }."#
		);
	}
}
