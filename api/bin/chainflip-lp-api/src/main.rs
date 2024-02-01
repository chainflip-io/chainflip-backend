use cf_primitives::{AccountId, BlockNumber, EgressId};
use cf_utilities::{
	rpc::NumberOrHex,
	task_scope::{task_scope, Scope},
	try_parse_number_or_hex,
};
use chainflip_api::{
	self,
	lp::{
		types::{LimitOrder, RangeOrder},
		ApiWaitForResult, LpApi, Order, Tick,
	},
	primitives::{
		chains::{Bitcoin, Ethereum, Polkadot},
		AccountRole, Asset, ForeignChain, Hash, RedemptionAmount,
	},
	settings::StateChain,
	BlockInfo, ChainApi, EthereumAddress, OperatorApi, SignedExtrinsicApi, StateChainApi,
	StorageApi, WaitFor,
};
use clap::Parser;
use custom_rpc::{
	CustomApiClient, RpcAsset,
	RpcAsset::{ExplicitChain, ImplicitChain},
};
use futures::{try_join, FutureExt, StreamExt};
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	server::ServerBuilder,
	types::SubscriptionResult,
	SubscriptionSink,
};
use pallet_cf_pools::{AssetPair, AssetsMap, IncreaseOrDecrease, OrderId, RangeOrderSize};
use rpc_types::{AssetBalance, OpenSwapChannels, OrderIdJson, RangeOrderSizeJson};
use sp_core::U256;
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	ops::Range,
	path::PathBuf,
	sync::Arc,
};
use tracing::log;

/// Contains RPC interface types that differ from internal types.
pub mod rpc_types {
	use super::*;
	use anyhow::anyhow;
	use cf_utilities::rpc::NumberOrHex;
	use chainflip_api::queries::SwapChannelInfo;
	use pallet_cf_pools::AssetsMap;
	use serde::{Deserialize, Serialize};

	#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
	pub struct OrderIdJson(NumberOrHex);
	impl TryFrom<OrderIdJson> for OrderId {
		type Error = anyhow::Error;

		fn try_from(value: OrderIdJson) -> Result<Self, Self::Error> {
			value.0.try_into().map_err(|_| anyhow!("Failed to convert order id to u64"))
		}
	}

	#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
	pub enum RangeOrderSizeJson {
		AssetAmounts { maximum: AssetsMap<NumberOrHex>, minimum: AssetsMap<NumberOrHex> },
		Liquidity { liquidity: NumberOrHex },
	}
	impl TryFrom<RangeOrderSizeJson> for RangeOrderSize {
		type Error = anyhow::Error;

		fn try_from(value: RangeOrderSizeJson) -> Result<Self, Self::Error> {
			Ok(match value {
				RangeOrderSizeJson::AssetAmounts { maximum, minimum } =>
					RangeOrderSize::AssetAmounts {
						maximum: maximum
							.try_map(TryInto::try_into)
							.map_err(|_| anyhow!("Failed to convert maximums to u128"))?,
						minimum: minimum
							.try_map(TryInto::try_into)
							.map_err(|_| anyhow!("Failed to convert minimums to u128"))?,
					},
				RangeOrderSizeJson::Liquidity { liquidity } => RangeOrderSize::Liquidity {
					liquidity: liquidity
						.try_into()
						.map_err(|_| anyhow!("Failed to convert liquidity to u128"))?,
				},
			})
		}
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct OpenSwapChannels {
		pub ethereum: Vec<SwapChannelInfo<Ethereum>>,
		pub bitcoin: Vec<SwapChannelInfo<Bitcoin>>,
		pub polkadot: Vec<SwapChannelInfo<Polkadot>>,
	}

	#[derive(Serialize, Deserialize, Clone)]
	pub struct AssetBalance {
		pub asset: Asset,
		pub balance: NumberOrHex,
	}
}

#[rpc(server, client, namespace = "lp")]
pub trait Rpc {
	#[method(name = "register_account")]
	async fn register_account(&self) -> RpcResult<Hash>;

	#[method(name = "liquidity_deposit")]
	async fn request_liquidity_deposit_address(
		&self,
		asset: RpcAsset,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<String>>;

	#[method(name = "register_liquidity_refund_address")]
	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> RpcResult<Hash>;

	#[method(name = "withdraw_asset")]
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: RpcAsset,
		destination_address: &str,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>>;

	#[method(name = "update_range_order")]
	async fn update_range_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>>;

	#[method(name = "set_range_order")]
	async fn set_range_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>>;

	#[method(name = "update_limit_order")]
	async fn update_limit_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "set_limit_order")]
	async fn set_limit_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
		id: OrderIdJson,
		tick: Option<Tick>,
		sell_amount: NumberOrHex,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "asset_balances")]
	async fn asset_balances(&self) -> RpcResult<BTreeMap<ForeignChain, Vec<AssetBalance>>>;

	#[method(name = "get_open_swap_channels")]
	async fn get_open_swap_channels(&self) -> RpcResult<OpenSwapChannels>;

	#[method(name = "request_redemption")]
	async fn request_redemption(
		&self,
		redeem_address: EthereumAddress,
		exact_amount: Option<NumberOrHex>,
		executor_address: Option<EthereumAddress>,
	) -> RpcResult<Hash>;

	#[subscription(name = "subscribe_order_fills", item = OrderFills)]
	fn subscribe_order_fills(&self);

	#[method(name = "order_fills")]
	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<OrderFills>;
}

pub struct RpcServerImpl {
	api: StateChainApi,
}

impl RpcServerImpl {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		LPOptions { ws_endpoint, signing_key_file, .. }: LPOptions,
	) -> Result<Self, anyhow::Error> {
		Ok(Self {
			api: StateChainApi::connect(scope, StateChain { ws_endpoint, signing_key_file })
				.await?,
		})
	}
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct OrderFills {
	block_hash: Hash,
	block_number: BlockNumber,
	fills: Vec<OrderFilled>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum OrderFilled {
	LimitOrder {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Order,
		id: U256,
		tick: Tick,
		sold: U256,
		bought: U256,
		fees: U256,
		remaining: U256,
	},
	RangeOrder {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		id: U256,
		range: Range<Tick>,
		fees: AssetsMap<U256>,
		liquidity: U256,
	},
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	/// Returns a deposit address
	async fn request_liquidity_deposit_address(
		&self,
		asset: RpcAsset,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<String>> {
		Ok(self
			.api
			.lp_api()
			.request_liquidity_deposit_address(asset.try_into()?, wait_for.unwrap_or_default())
			.await
			.map(|result| result.map_details(|address| address.to_string()))?)
	}

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> RpcResult<Hash> {
		let ewa_address = chainflip_api::clean_foreign_chain_address(chain, address)?;
		Ok(self.api.lp_api().register_liquidity_refund_address(ewa_address).await?)
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: RpcAsset,
		destination_address: &str,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>> {
		let asset: Asset = asset.try_into()?;

		let destination_address =
			chainflip_api::clean_foreign_chain_address(asset.into(), destination_address)?;

		Ok(self
			.api
			.lp_api()
			.withdraw_asset(
				try_parse_number_or_hex(amount)?,
				asset,
				destination_address,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	/// Returns a list of all assets and their free balance in json format
	async fn asset_balances(&self) -> RpcResult<BTreeMap<ForeignChain, Vec<AssetBalance>>> {
		let cf_asset_balances = self
			.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_asset_balances(
				self.api.state_chain_client.account_id(),
				Some(self.api.state_chain_client.latest_finalized_block().hash),
			)
			.await?;

		let mut lp_asset_balances: BTreeMap<ForeignChain, Vec<AssetBalance>> = BTreeMap::new();

		for rpc_asset_with_amount in cf_asset_balances {
			let (chain, asset) = match rpc_asset_with_amount.asset {
				ImplicitChain(asset) => (asset.into(), asset),
				ExplicitChain { chain, asset } => (chain, asset),
			};
			let asset_balance =
				AssetBalance { asset, balance: rpc_asset_with_amount.amount.into() };
			lp_asset_balances.entry(chain).or_insert_with(Vec::new).push(asset_balance);
		}
		Ok(lp_asset_balances)
	}

	async fn update_range_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>> {
		Ok(self
			.api
			.lp_api()
			.update_range_order(
				base_asset.try_into()?,
				quote_asset.try_into()?,
				id.try_into()?,
				tick_range,
				size_change.try_map(|size| size.try_into())?,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn set_range_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>> {
		Ok(self
			.api
			.lp_api()
			.set_range_order(
				base_asset.try_into()?,
				quote_asset.try_into()?,
				id.try_into()?,
				tick_range,
				size.try_into()?,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn update_limit_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(self
			.api
			.lp_api()
			.update_limit_order(
				base_asset.try_into()?,
				quote_asset.try_into()?,
				side,
				id.try_into()?,
				tick,
				amount_change.try_map(try_parse_number_or_hex)?,
				dispatch_at,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn set_limit_order(
		&self,
		base_asset: RpcAsset,
		quote_asset: RpcAsset,
		side: Order,
		id: OrderIdJson,
		tick: Option<Tick>,
		sell_amount: NumberOrHex,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(self
			.api
			.lp_api()
			.set_limit_order(
				base_asset.try_into()?,
				quote_asset.try_into()?,
				side,
				id.try_into()?,
				tick,
				try_parse_number_or_hex(sell_amount)?,
				dispatch_at,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	/// Returns the tx hash that the account role was set
	async fn register_account(&self) -> RpcResult<Hash> {
		Ok(self
			.api
			.operator_api()
			.register_account_role(AccountRole::LiquidityProvider)
			.await?)
	}

	async fn get_open_swap_channels(&self) -> RpcResult<OpenSwapChannels> {
		let api = self.api.query_api();

		let (ethereum, bitcoin, polkadot) = tokio::try_join!(
			api.get_open_swap_channels::<Ethereum>(None),
			api.get_open_swap_channels::<Bitcoin>(None),
			api.get_open_swap_channels::<Polkadot>(None),
		)?;
		Ok(OpenSwapChannels { ethereum, bitcoin, polkadot })
	}

	async fn request_redemption(
		&self,
		redeem_address: EthereumAddress,
		exact_amount: Option<NumberOrHex>,
		executor_address: Option<EthereumAddress>,
	) -> RpcResult<Hash> {
		let redeem_amount = if let Some(number_or_hex) = exact_amount {
			RedemptionAmount::Exact(try_parse_number_or_hex(number_or_hex)?)
		} else {
			RedemptionAmount::Max
		};

		Ok(self
			.api
			.operator_api()
			.request_redemption(redeem_amount, redeem_address, executor_address)
			.await?)
	}

	fn subscribe_order_fills(&self, mut sink: SubscriptionSink) -> SubscriptionResult {
		sink.accept()?;
		let state_chain_client = self.api.state_chain_client.clone();
		tokio::spawn(async move {
			let mut finalized_block_stream = state_chain_client.finalized_block_stream().await;
			while let Some(block) = finalized_block_stream.next().await {
				if let Err(option_error) = order_fills(state_chain_client.clone(), block)
					.await
					.map_err(Some)
					.and_then(|order_fills| match sink.send(&order_fills) {
						Ok(true) => Ok(()),
						Ok(false) => Err(None),
						Err(error) => Err(Some(jsonrpsee::core::Error::ParseError(error))),
					}) {
					if let Some(error) = option_error {
						sink.close(error);
					}
					break
				}
			}
		});

		Ok(())
	}

	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<OrderFills> {
		let state_chain_client = &self.api.state_chain_client;

		let block = if let Some(at) = at {
			state_chain_client.block(at).await?
		} else {
			state_chain_client.latest_finalized_block()
		};

		Ok(order_fills(state_chain_client.clone(), block).await?)
	}
}

async fn order_fills<StateChainClient>(
	state_chain_client: Arc<StateChainClient>,
	block: BlockInfo,
) -> Result<OrderFills, jsonrpsee::core::Error>
where
	StateChainClient: StorageApi,
{
	Ok(OrderFills {
		block_hash: block.hash,
		block_number: block.number,
		fills: {
			let (previous_pools, pools, events) = try_join!(
				state_chain_client.storage_map::<pallet_cf_pools::Pools<
					chainflip_api::primitives::state_chain_runtime::Runtime,
				>, HashMap<_, _>>(block.parent_hash),
				state_chain_client.storage_map::<pallet_cf_pools::Pools<
					chainflip_api::primitives::state_chain_runtime::Runtime,
				>, HashMap<_, _>>(block.hash),
				state_chain_client.storage_value::<frame_system::Events<
					chainflip_api::primitives::state_chain_runtime::Runtime,
				>>(block.hash)
			)?;

			let updated_range_orders = events.iter().filter_map(|event_record| {
				match &event_record.event {
					chainflip_api::primitives::state_chain_runtime::RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::RangeOrderUpdated {
						lp,
						base_asset,
						quote_asset,
						id,
						..
					}) => {
						Some((lp.clone(), AssetPair::new(*base_asset, *quote_asset).unwrap(), *id))
					},
					_ => {
						None
					}
				}
			}).collect::<HashSet<_>>();

			let updated_limit_orders = events.iter().filter_map(|event_record| {
				match &event_record.event {
					chainflip_api::primitives::state_chain_runtime::RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::LimitOrderUpdated {
						lp,
						base_asset,
						quote_asset,
						side,
						id,
						..
					}) => {
						Some((lp.clone(), AssetPair::new(*base_asset, *quote_asset).unwrap(), *side, *id))
					},
					_ => {
						None
					}
				}
			}).collect::<HashSet<_>>();

			let order_fills = pools
				.iter()
				.flat_map(|(asset_pair, pool)| {
					let updated_range_orders = &updated_range_orders;
					let updated_limit_orders = &updated_limit_orders;
					let previous_pools = &previous_pools;
					[Order::Sell, Order::Buy]
						.into_iter()
						.flat_map(move |side| {
							pool.pool_state.limit_orders(side).filter_map(
								move |((lp, id), tick, collected, position_info)| {
									let (fees, sold, bought) = {
										let option_previous_order_state = if updated_limit_orders
											.contains(&(lp.clone(), *asset_pair, side, id))
										{
											None
										} else {
											previous_pools.get(asset_pair).and_then(|pool| {
												pool.pool_state
													.limit_order(&(lp.clone(), id), side, tick)
													.ok()
											})
										};

										if let Some((previous_collected, _)) =
											option_previous_order_state
										{
											(
												collected.fees - previous_collected.fees,
												collected.sold_amount -
													previous_collected.sold_amount,
												collected.bought_amount -
													previous_collected.bought_amount,
											)
										} else {
											(
												collected.fees,
												collected.sold_amount,
												collected.bought_amount,
											)
										}
									};

									if fees.is_zero() && sold.is_zero() && bought.is_zero() {
										None
									} else {
										Some(OrderFilled::LimitOrder {
											lp,
											base_asset: asset_pair.assets().base,
											quote_asset: asset_pair.assets().quote,
											side,
											id: id.into(),
											tick,
											sold,
											bought,
											fees,
											remaining: position_info.amount,
										})
									}
								},
							)
						})
						.chain(pool.pool_state.range_orders().filter_map(
							move |((lp, id), range, collected, position_info)| {
								let fees = {
									let option_previous_order_state = if updated_range_orders
										.contains(&(lp.clone(), *asset_pair, id))
									{
										None
									} else {
										previous_pools.get(asset_pair).and_then(|pool| {
											pool.pool_state
												.range_order(&(lp.clone(), id), range.clone())
												.ok()
										})
									};

									if let Some((previous_collected, _)) =
										option_previous_order_state
									{
										collected
											.fees
											.zip(previous_collected.fees)
											.map(|_, (fees, previous_fees)| fees - previous_fees)
									} else {
										collected.fees
									}
								};

								if fees == Default::default() {
									None
								} else {
									Some(OrderFilled::RangeOrder {
										lp: lp.clone(),
										base_asset: asset_pair.assets().base,
										quote_asset: asset_pair.assets().quote,
										id: id.into(),
										range: range.clone(),
										fees: fees.map(|_, fees| fees).into(),
										liquidity: position_info.liquidity.into(),
									})
								}
							},
						))
				})
				.collect::<Vec<_>>();

			order_fills
		},
	})
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), version_short = 'v')]
pub struct LPOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the LP server will listen for connections. Use 0 to assign a random port."
	)]
	pub port: u16,
	#[clap(
		long = "state_chain.ws_endpoint",
		default_value = "ws://localhost:9944",
		help = "The state chain node's RPC endpoint."
	)]
	pub ws_endpoint: String,
	#[clap(
		long = "state_chain.signing_key_file",
		default_value = "/etc/chainflip/keys/signing_key_file",
		help = "A path to a file that contains the LP's secret key for signing extrinsics."
	)]
	pub signing_key_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let opts = LPOptions::parse();
	chainflip_api::use_chainflip_account_id_encoding();
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	assert!(
		opts.signing_key_file.exists(),
		"No signing_key_file found at {}",
		opts.signing_key_file.to_string_lossy()
	);

	task_scope(|scope| {
		async move {
			let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
			let server_addr = server.local_addr()?;
			let server = server.start(RpcServerImpl::new(scope, opts).await?.into_rpc())?;

			log::info!("🎙 Server is listening on {server_addr}.");

			server.stopped().await;
			Ok(())
		}
		.boxed()
	})
	.await
}
