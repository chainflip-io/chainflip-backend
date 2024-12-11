use anyhow::anyhow;
use cf_primitives::{BasisPoints, BlockNumber, EgressId};
use cf_utilities::{
	health::{self, HealthCheckOptions},
	rpc::NumberOrHex,
	task_scope::{task_scope, Scope},
	try_parse_number_or_hex,
};
use chainflip_api::{
	self,
	lp::{
		types::{LimitOrRangeOrder, LimitOrder, RangeOrder},
		ApiWaitForResult, LpApi, Side, Tick,
	},
	primitives::{
		chains::{assets::any::AssetMap, Bitcoin, Ethereum, Polkadot},
		AccountRole, Asset, ForeignChain, Hash, RedemptionAmount,
	},
	settings::StateChain,
	AccountId32, AddressString, BlockUpdate, ChainApi, EthereumAddress, OperatorApi,
	SignedExtrinsicApi, StateChainApi, WaitFor,
};
use clap::Parser;
use custom_rpc::{order_fills::OrderFills, CustomApiClient};
use futures::{stream, FutureExt, StreamExt};
use jsonrpsee::{
	core::{async_trait, ClientError},
	proc_macros::rpc,
	server::ServerBuilder,
	types::{ErrorCode, ErrorObject, ErrorObjectOwned},
	PendingSubscriptionSink,
};
use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, OrderId, RangeOrderSize, MAX_ORDERS_DELETE};
use rpc_types::{OpenSwapChannels, OrderIdJson, RangeOrderSizeJson};
use sp_core::{bounded::BoundedVec, ConstU32, H256, U256};
use std::{
	ops::Range,
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc},
};
use tracing::log;

/// Contains RPC interface types that differ from internal types.
pub mod rpc_types {
	use super::*;
	use anyhow::anyhow;
	use cf_utilities::rpc::NumberOrHex;
	use chainflip_api::{lp::PoolPairsMap, queries::SwapChannelInfo};
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
		AssetAmounts { maximum: PoolPairsMap<NumberOrHex>, minimum: PoolPairsMap<NumberOrHex> },
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
}

#[rpc(server, client, namespace = "lp")]
pub trait Rpc {
	#[method(name = "register_account")]
	async fn register_account(&self) -> RpcResult<Hash>;

	#[method(name = "liquidity_deposit")]
	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<String>>;

	#[method(name = "register_liquidity_refund_address")]
	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> RpcResult<Hash>;

	#[method(name = "withdraw_asset")]
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: AddressString,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>>;

	#[method(name = "transfer_asset")]
	async fn transfer_asset(
		&self,
		amount: U256,
		asset: Asset,
		destination_account: AccountId32,
	) -> RpcResult<Hash>;

	#[method(name = "update_range_order")]
	async fn update_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>>;

	#[method(name = "set_range_order")]
	async fn set_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>>;

	#[method(name = "update_limit_order")]
	async fn update_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "set_limit_order")]
	async fn set_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderIdJson,
		tick: Option<Tick>,
		sell_amount: NumberOrHex,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "free_balances", aliases = ["lp_asset_balances"])]
	async fn free_balances(&self) -> RpcResult<AssetMap<U256>>;

	#[method(name = "get_open_swap_channels")]
	async fn get_open_swap_channels(&self) -> RpcResult<OpenSwapChannels>;

	#[method(name = "request_redemption")]
	async fn request_redemption(
		&self,
		redeem_address: EthereumAddress,
		exact_amount: Option<NumberOrHex>,
		executor_address: Option<EthereumAddress>,
	) -> RpcResult<Hash>;

	#[subscription(name = "subscribe_order_fills", item = BlockUpdate<OrderFills>)]
	async fn subscribe_order_fills(&self);

	#[method(name = "order_fills")]
	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<BlockUpdate<OrderFills>>;

	#[method(name = "cancel_all_orders")]
	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>>;

	#[method(name = "cancel_orders_batch")]
	async fn cancel_orders_batch(
		&self,
		orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>>;
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

#[derive(thiserror::Error, Debug)]
pub enum LpApiError {
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
	#[error(transparent)]
	ClientError(#[from] jsonrpsee::core::ClientError),
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

type RpcResult<T> = Result<T, LpApiError>;

impl From<LpApiError> for ErrorObjectOwned {
	fn from(error: LpApiError) -> Self {
		match error {
			LpApiError::ErrorObject(error) => error,
			LpApiError::ClientError(error) => match error {
				ClientError::Call(obj) => obj,
				internal => {
					log::error!("Internal rpc client error: {internal:?}");
					ErrorObject::owned(
						ErrorCode::InternalError.code(),
						"Internal rpc client error",
						None::<()>,
					)
				},
			},
			LpApiError::Other(error) => jsonrpsee::types::error::ErrorObjectOwned::owned(
				ErrorCode::ServerError(0xcf).code(),
				error.to_string(),
				None::<()>,
			),
		}
	}
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	/// Returns a deposit address
	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<String>> {
		Ok(self
			.api
			.lp_api()
			.request_liquidity_deposit_address(asset, wait_for.unwrap_or_default(), boost_fee)
			.await
			.map(|result| result.map_details(|address| address.to_string()))?)
	}

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> RpcResult<Hash> {
		Ok(self.api.lp_api().register_liquidity_refund_address(chain, address).await?)
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: AddressString,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>> {
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

	/// Returns an egress id
	async fn transfer_asset(
		&self,
		amount: U256,
		asset: Asset,
		destination_account: AccountId32,
	) -> RpcResult<H256> {
		Ok(self
			.api
			.lp_api()
			.transfer_asset(
				amount.try_into().map_err(|_| anyhow!("Failed to convert amount to u128"))?,
				asset,
				destination_account,
			)
			.await?)
	}

	/// Returns a list of all assets and their free balance in json format
	async fn free_balances(&self) -> RpcResult<AssetMap<U256>> {
		Ok(self
			.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_free_balances(
				self.api.state_chain_client.account_id(),
				Some(self.api.state_chain_client.latest_finalized_block().hash),
			)
			.await?)
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>> {
		Ok(self
			.api
			.lp_api()
			.update_range_order(
				base_asset,
				quote_asset,
				id.try_into()?,
				tick_range,
				size_change.try_map(|size| size.try_into())?,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>> {
		Ok(self
			.api
			.lp_api()
			.set_range_order(
				base_asset,
				quote_asset,
				id.try_into()?,
				tick_range,
				size.try_into()?,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn update_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
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
				base_asset,
				quote_asset,
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
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
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
				base_asset,
				quote_asset,
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

	async fn subscribe_order_fills(&self, pending_sink: PendingSubscriptionSink) {
		// pipe results from custom-rpc subscription
		match self.api.raw_client().cf_subscribe_lp_order_fills().await {
			Ok(subscription) => {
				let stream = stream::unfold(subscription, move |mut sub| async move {
					match sub.next().await {
						Some(Ok(block_update)) => Some((block_update, sub)),
						_ => None,
					}
				})
				.boxed();

				tokio::spawn(async move {
					sc_rpc::utils::pipe_from_stream(pending_sink, stream).await;
				});
			},
			Err(e) => {
				pending_sink.reject(LpApiError::ClientError(e)).await;
			},
		}
	}

	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<BlockUpdate<OrderFills>> {
		self.api
			.raw_client()
			.cf_lp_get_order_fills(at)
			.await
			.map_err(LpApiError::ClientError)
	}

	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>> {
		let mut orders_to_delete: Vec<CloseOrder> = vec![];
		let pool_pairs = self
			.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_available_pools(None)
			.await?;
		for pool in pool_pairs {
			let orders = self
				.api
				.state_chain_client
				.base_rpc_client
				.raw_rpc_client
				.cf_pool_orders(
					pool.base,
					pool.quote,
					Some(self.api.state_chain_client.account_id()),
					None,
					None,
				)
				.await?;
			for order in orders.range_orders {
				orders_to_delete.push(CloseOrder::Range {
					base_asset: pool.base,
					quote_asset: pool.quote,
					id: order.id.try_into().expect("Internal AMM OrderId is a u64"),
				});
			}
			for order in orders.limit_orders.asks {
				orders_to_delete.push(CloseOrder::Limit {
					base_asset: pool.base,
					quote_asset: pool.quote,
					side: Side::Sell,
					id: order.id.try_into().expect("Internal AMM OrderId is a u64"),
				});
			}
			for order in orders.limit_orders.bids {
				orders_to_delete.push(CloseOrder::Limit {
					base_asset: pool.base,
					quote_asset: pool.quote,
					side: Side::Buy,
					id: order.id.try_into().expect("Internal AMM OrderId is a u64"),
				});
			}
		}

		// in case there are more than 100 elements we need to split the orders into chunks of 100
		// and submit multiple extrinsics
		let mut extrinsic_submissions = vec![];
		for order_chunk in orders_to_delete.chunks(MAX_ORDERS_DELETE as usize) {
			extrinsic_submissions.push(
				self.api
					.lp_api()
					.cancel_orders_batch(
						BoundedVec::<_, ConstU32<MAX_ORDERS_DELETE>>::try_from(
							order_chunk.to_vec(),
						)
						.expect("Guaranteed by `chunk` method."),
						wait_for.unwrap_or_default(),
					)
					.await?,
			);
		}

		Ok(extrinsic_submissions)
	}

	async fn cancel_orders_batch(
		&self,
		orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>> {
		Ok(self
			.api
			.lp_api()
			.cancel_orders_batch(orders, wait_for.unwrap_or_default())
			.await?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), short_flag = 'v')]
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
	#[clap(flatten)]
	pub health_check: HealthCheckOptions,
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
			// initialize healthcheck endpoint
			let has_completed_initialising = Arc::new(AtomicBool::new(false));
			health::start_if_configured(
				scope,
				&opts.health_check,
				has_completed_initialising.clone(),
			)
			.await?;

			let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
			let server_addr = server.local_addr()?;
			let server = server.start(RpcServerImpl::new(scope, opts).await?.into_rpc());

			log::info!("🎙 Server is listening on {server_addr}.");

			// notify healthcheck completed
			has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

			server.stopped().await;
			Ok(())
		}
		.boxed()
	})
	.await
}
