use cf_amm::{
	common::{Amount, Price, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	btc::BitcoinNetwork,
	dot::PolkadotHash,
	eth::Address as EthereumAddress,
};
use cf_primitives::{AccountRole, Asset, AssetAmount, ForeignChain, SemVer, SwapOutput};
use core::ops::Range;
use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{CallError, SubscriptionEmptyError},
	SubscriptionSink,
};
use pallet_cf_governance::GovCallHash;
use pallet_cf_pools::{AssetsMap, Depth, PoolInfo, PoolLiquidity, PoolOrders};
use sc_client_api::{BlockchainEvents, HeaderBackend};
use serde::{Deserialize, Serialize};
use sp_api::BlockT;
use sp_core::H160;
use sp_rpc::number::NumberOrHex;
use sp_runtime::DispatchError;
use state_chain_runtime::{
	chainflip::{ChainAddressConverter, Offence},
	constants::common::TX_FEE_MULTIPLIER,
	runtime_apis::{CustomRuntimeApi, Environment, LiquidityProviderInfo, RuntimeApiAccountInfoV2},
};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};

#[derive(Serialize, Deserialize)]
#[serde(tag = "role")]
#[serde(rename_all = "snake_case")]
pub enum RpcAccountInfo {
	None,
	Broker,
	LiquidityProvider {
		balances: HashMap<ForeignChain, HashMap<Asset, NumberOrHex>>,
		refund_addresses: HashMap<ForeignChain, Option<EncodedAddress>>,
	},
	Validator {
		balance: NumberOrHex,
		bond: NumberOrHex,
		last_heartbeat: u32,
		online_credits: u32,
		reputation_points: i32,
		keyholder_epochs: Vec<u32>,
		is_current_authority: bool,
		is_current_backup: bool,
		is_qualified: bool,
		is_online: bool,
		is_bidding: bool,
		bound_redeem_address: Option<EthereumAddress>,
	},
}

#[test]
fn test_account_info_serialization() {
	assert_eq!(serde_json::to_string(&RpcAccountInfo::none()).unwrap(), r#"{"role":"none"}"#);
	assert_eq!(serde_json::to_string(&RpcAccountInfo::broker()).unwrap(), r#"{"role":"broker"}"#);

	let lp = RpcAccountInfo::lp(LiquidityProviderInfo {
		refund_addresses: vec![
			(
				ForeignChain::Ethereum,
				Some(cf_chains::ForeignChainAddress::Eth(H160::from([1; 20]))),
			),
			(ForeignChain::Bitcoin, None),
		],
		balances: vec![(Asset::Eth, u128::MAX), (Asset::Btc, 0)],
	});

	assert_eq!(
		serde_json::to_string(&lp).unwrap(),
		r#"{"role":"liquidity_provider","balances":{"Bitcoin":{"Btc":"0xffffffffffffffffffffffffffffffff"}},"refund_addresses":{"Bitcoin":null}}"#
	);
}

impl RpcAccountInfo {
	fn none() -> Self {
		Self::None
	}

	fn broker() -> Self {
		Self::Broker
	}

	fn lp(info: LiquidityProviderInfo) -> Self {
		let mut balances = HashMap::new();

		for (asset, balance) in info.balances {
			balances
				.entry(asset.into())
				.or_insert_with(HashMap::new)
				.insert(asset, balance.into());
		}

		Self::LiquidityProvider {
			balances,
			refund_addresses: info
				.refund_addresses
				.into_iter()
				.map(|(chain, address)| {
					(chain, address.map(ChainAddressConverter::to_encoded_address))
				})
				.collect(),
		}
	}

	fn validator(info: RuntimeApiAccountInfoV2) -> Self {
		Self::Validator {
			balance: info.balance.into(),
			bond: info.bond.into(),
			last_heartbeat: info.last_heartbeat,
			online_credits: info.online_credits,
			reputation_points: info.reputation_points,
			keyholder_epochs: info.keyholder_epochs,
			is_current_authority: info.is_current_authority,
			is_current_backup: info.is_current_backup,
			is_qualified: info.is_qualified,
			is_online: info.is_online,
			is_bidding: info.is_bidding,
			bound_redeem_address: info.bound_redeem_address,
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct RpcAccountInfoV2 {
	pub balance: NumberOrHex,
	pub bond: NumberOrHex,
	pub last_heartbeat: u32,
	pub online_credits: u32,
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<u32>,
	pub is_current_authority: bool,
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EthereumAddress>,
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
			intermediary: swap_output.intermediary.map(NumberOrHex::from),
			output: NumberOrHex::from(swap_output.output),
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct RpcEnvironment {
	bitcoin_network: BitcoinNetwork,
	ethereum_chain_id: cf_chains::evm::api::EvmChainId,
	polkadot_genesis_hash: PolkadotHash,
}

impl From<Environment> for RpcEnvironment {
	fn from(environment: Environment) -> Self {
		Self {
			bitcoin_network: environment.bitcoin_network,
			ethereum_chain_id: environment.ethereum_chain_id,
			polkadot_genesis_hash: environment.polkadot_genesis_hash,
		}
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
	) -> RpcResult<RpcAccountInfo>;
	#[method(name = "account_info_v2")]
	fn cf_account_info_v2(
		&self,
		account_id: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcAccountInfoV2>;
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
		from: Asset,
		to: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<Price>>;
	#[method(name = "swap_rate")]
	fn cf_pool_swap_rate(
		&self,
		from: Asset,
		to: Asset,
		amount: NumberOrHex,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutput>;
	#[method(name = "required_asset_ratio_for_range_order")]
	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<AssetsMap<Amount>>>;
	#[method(name = "pool_info")]
	fn cf_pool_info(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolInfo>>;
	#[method(name = "pool_depth")]
	fn cf_pool_depth(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<AssetsMap<Depth>>>;
	#[method(name = "pool_liquidity")]
	fn cf_pool_liquidity(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolLiquidity>>;
	#[method(name = "pool_orders")]
	fn cf_pool_orders(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		lp: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolOrders>>;
	#[method(name = "pool_range_order_liquidity_value")]
	fn cf_pool_range_order_liquidity_value(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<AssetsMap<Amount>>>;
	#[method(name = "environment")]
	fn cf_environment(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEnvironment>;
	#[method(name = "current_compatibility_version")]
	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer>;
	#[method(name = "min_swap_amount")]
	fn cf_min_swap_amount(&self, asset: Asset) -> RpcResult<AssetAmount>;
	#[subscription(name = "subscribe_pool_price", item = Price)]
	fn cf_subscribe_pool_price(&self, from: Asset, to: Asset);

	#[subscription(name = "subscribe_prewitness_swaps", item = Vec<AssetAmount>)]
	fn cf_subscribe_prewitness_swaps(&self, from: Asset, to: Asset);

	#[method(name = "prewitness_swaps")]
	fn cf_prewitness_swaps(
		&self,
		from: Asset,
		to: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<Vec<AssetAmount>>>;
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

fn map_dispatch_error(e: DispatchError) -> jsonrpsee::core::Error {
	jsonrpsee::core::Error::from(anyhow::anyhow!("Dispatch error: {}", <&'static str>::from(e)))
}

impl<C, B> CustomApiServer for CustomRpc<C, B>
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

		Ok(
			match api
				.cf_account_role(self.unwrap_or_best(at), account_id.clone())
				.map_err(to_rpc_error)?
				.unwrap_or(AccountRole::None)
			{
				AccountRole::None => RpcAccountInfo::none(),
				AccountRole::Broker => RpcAccountInfo::broker(),
				AccountRole::LiquidityProvider => {
					let info = api
						.cf_liquidity_provider_info(self.unwrap_or_best(at), account_id)
						.map_err(to_rpc_error)?
						.expect("role already validated");

					RpcAccountInfo::lp(info)
				},
				AccountRole::Validator => {
					let info = api
						.cf_account_info_v2(self.unwrap_or_best(at), account_id)
						.map_err(to_rpc_error)?;

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
			.cf_account_info_v2(self.unwrap_or_best(at), account_id)
			.map_err(to_rpc_error)?;

		Ok(RpcAccountInfoV2 {
			balance: account_info.balance.into(),
			bond: account_info.bond.into(),
			last_heartbeat: account_info.last_heartbeat,
			online_credits: account_info.online_credits,
			reputation_points: account_info.reputation_points,
			keyholder_epochs: account_info.keyholder_epochs,
			is_current_authority: account_info.is_current_authority,
			is_current_backup: account_info.is_current_backup,
			is_qualified: account_info.is_qualified,
			is_online: account_info.is_online,
			is_bidding: account_info.is_bidding,
			bound_redeem_address: account_info.bound_redeem_address,
		})
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
		})
	}

	fn cf_pool_price(
		&self,
		from: Asset,
		to: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<Price>> {
		self.client
			.runtime_api()
			.cf_pool_price(self.unwrap_or_best(at), from, to)
			.map_err(to_rpc_error)
	}

	fn cf_pool_swap_rate(
		&self,
		from: Asset,
		to: Asset,
		amount: NumberOrHex,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcSwapOutput> {
		self.client
			.runtime_api()
			.cf_pool_simulate_swap(
				self.unwrap_or_best(at),
				from,
				to,
				cf_utilities::try_parse_number_or_hex(amount).and_then(|amount| {
					if amount == 0 {
						Err(anyhow::anyhow!("Swap input amount cannot be zero."))
					} else {
						Ok(amount)
					}
				})?,
			)
			.map_err(to_rpc_error)
			.and_then(|r| {
				r.ok_or(jsonrpsee::core::Error::from(anyhow::anyhow!("Unable to process swap.")))
			})
			.map(RpcSwapOutput::from)
	}

	fn cf_pool_info(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolInfo>> {
		self.client
			.runtime_api()
			.cf_pool_info(self.unwrap_or_best(at), base_asset, pair_asset)
			.map_err(to_rpc_error)
	}

	fn cf_pool_depth(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		tick_range: Range<Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<AssetsMap<Depth>>> {
		self.client
			.runtime_api()
			.cf_pool_depth(self.unwrap_or_best(at), base_asset, pair_asset, tick_range)
			.map_err(to_rpc_error)?
			.transpose()
			.map_err(map_dispatch_error)
	}

	fn cf_pool_liquidity(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolLiquidity>> {
		self.client
			.runtime_api()
			.cf_pool_liquidity(self.unwrap_or_best(at), base_asset, pair_asset)
			.map_err(to_rpc_error)
	}

	fn cf_required_asset_ratio_for_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		tick_range: Range<cf_amm::common::Tick>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<AssetsMap<Amount>>> {
		self.client
			.runtime_api()
			.cf_required_asset_ratio_for_range_order(
				self.unwrap_or_best(at),
				base_asset,
				pair_asset,
				tick_range,
			)
			.map_err(to_rpc_error)?
			.transpose()
			.map_err(map_dispatch_error)
	}

	fn cf_pool_orders(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		lp: state_chain_runtime::AccountId,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<PoolOrders>> {
		self.client
			.runtime_api()
			.cf_pool_orders(self.unwrap_or_best(at), base_asset, pair_asset, lp)
			.map_err(to_rpc_error)
	}

	fn cf_pool_range_order_liquidity_value(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		tick_range: Range<Tick>,
		liquidity: Liquidity,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<AssetsMap<Amount>>> {
		self.client
			.runtime_api()
			.cf_pool_range_order_liquidity_value(
				self.unwrap_or_best(at),
				base_asset,
				pair_asset,
				tick_range,
				liquidity,
			)
			.map_err(to_rpc_error)?
			.transpose()
			.map_err(map_dispatch_error)
	}

	fn cf_environment(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEnvironment> {
		self.client
			.runtime_api()
			.cf_environment(self.unwrap_or_best(at))
			.map_err(to_rpc_error)
			.map(RpcEnvironment::from)
	}

	fn cf_current_compatibility_version(&self) -> RpcResult<SemVer> {
		self.client
			.runtime_api()
			.cf_current_compatibility_version(self.unwrap_or_best(None))
			.map_err(to_rpc_error)
	}

	fn cf_min_swap_amount(&self, asset: Asset) -> RpcResult<AssetAmount> {
		self.client
			.runtime_api()
			.cf_min_swap_amount(self.unwrap_or_best(None), asset)
			.map_err(to_rpc_error)
	}

	fn cf_subscribe_pool_price(
		&self,
		sink: SubscriptionSink,
		from: Asset,
		to: Asset,
	) -> Result<(), SubscriptionEmptyError> {
		self.new_update_subscription(sink, move |api, hash| api.cf_pool_price(hash, from, to))
	}

	fn cf_subscribe_prewitness_swaps(
		&self,
		sink: SubscriptionSink,
		from: Asset,
		to: Asset,
	) -> Result<(), SubscriptionEmptyError> {
		self.new_items_subscription(sink, move |api, hash| api.cf_prewitness_swaps(hash, from, to))
	}

	fn cf_prewitness_swaps(
		&self,
		from: Asset,
		to: Asset,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Option<Vec<AssetAmount>>> {
		self.client
			.runtime_api()
			.cf_prewitness_swaps(self.unwrap_or_best(at), from, to)
			.map_err(to_rpc_error)
	}
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
	/// Upon subscribing returns the first value immediately and then subscribes to updates. i.e. it
	/// will only return a value if it has changed from the previous value it returned.
	fn new_update_subscription<
		T: Serialize + Send + Clone + Eq + 'static,
		E: std::error::Error + Send + Sync + 'static,
		F: Fn(&C::Api, state_chain_runtime::Hash) -> Result<T, E> + Send + Clone + 'static,
	>(
		&self,
		mut sink: SubscriptionSink,
		f: F,
	) -> Result<(), SubscriptionEmptyError> {
		use futures::{future::FutureExt, stream::StreamExt};

		let client = self.client.clone();

		let initial = match f(&self.client.runtime_api(), self.client.info().best_hash) {
			Ok(initial) => initial,
			Err(e) => {
				let _ = sink.reject(jsonrpsee::core::Error::from(
					sc_rpc_api::state::error::Error::Client(Box::new(e)),
				));
				return Ok(())
			},
		};

		let mut previous = initial.clone();

		let stream = self
			.client
			.import_notification_stream()
			.filter(|n| futures::future::ready(n.is_new_best))
			.filter_map(move |n| {
				let new = f(&client.runtime_api(), n.hash);

				match new {
					Ok(new) if new != previous => {
						previous = new.clone();
						futures::future::ready(Some(new))
					},
					_ => futures::future::ready(None),
				}
			});

		let stream = futures::stream::once(futures::future::ready(initial)).chain(stream);

		let fut = async move {
			sink.pipe_from_stream(stream).await;
		};

		self.executor.spawn("cf-rpc-update-subscription", Some("rpc"), fut.boxed());

		Ok(())
	}

	/// After creating the subscription it will return all the new prewitnessed swaps from this
	/// point onwards.
	fn new_items_subscription<
		T: Serialize + Send + Clone + Eq + 'static,
		E: std::error::Error + Send + Sync + 'static,
		F: Fn(&C::Api, state_chain_runtime::Hash) -> Result<Option<T>, E> + Send + Clone + 'static,
	>(
		&self,
		mut sink: SubscriptionSink,
		f: F,
	) -> Result<(), SubscriptionEmptyError> {
		use futures::{future::FutureExt, stream::StreamExt};

		let client = self.client.clone();

		let stream = self
			.client
			.import_notification_stream()
			.filter(|n| futures::future::ready(n.is_new_best))
			.filter_map(move |n| {
				let new = f(&client.runtime_api(), n.hash);

				match new {
					Ok(new) => futures::future::ready(new),
					_ => futures::future::ready(None),
				}
			});

		let fut = async move {
			sink.pipe_from_stream(stream).await;
		};

		self.executor.spawn("cf-rpc-stream-subscription", Some("rpc"), fut.boxed());

		Ok(())
	}
}
