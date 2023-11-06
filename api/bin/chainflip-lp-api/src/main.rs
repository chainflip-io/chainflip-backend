use cf_utilities::{
	rpc::NumberOrHex,
	task_scope::{task_scope, Scope},
	try_parse_number_or_hex, AnyhowRpcError,
};
use chainflip_api::{
	self,
	lp::{LimitOrderReturn, LpApi, RangeOrderReturn, Tick},
	primitives::{
		chains::{Bitcoin, Ethereum, Polkadot},
		AccountRole, Asset, ForeignChain, Hash,
	},
	settings::StateChain,
	OperatorApi, StateChainApi,
};
use clap::Parser;
use custom_rpc::RpcAsset;
use futures::FutureExt;
use jsonrpsee::{core::async_trait, proc_macros::rpc, server::ServerBuilder};
use pallet_cf_pools::{IncreaseOrDecrease, OrderId, RangeOrderSize};
use rpc_types::{AssetBalance, OpenSwapChannels, OrderIdJson, RangeOrderSizeJson};
use std::{collections::BTreeMap, ops::Range, path::PathBuf};
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
		pub balance: u128,
	}
}

#[rpc(server, client, namespace = "lp")]
pub trait Rpc {
	#[method(name = "register_account")]
	async fn register_account(&self) -> Result<Hash, AnyhowRpcError>;

	#[method(name = "liquidity_deposit")]
	async fn request_liquidity_deposit_address(
		&self,
		asset: RpcAsset,
	) -> Result<String, AnyhowRpcError>;

	#[method(name = "register_liquidity_refund_address")]
	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> Result<Hash, AnyhowRpcError>;

	#[method(name = "withdraw_asset")]
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: RpcAsset,
		destination_address: &str,
	) -> Result<(ForeignChain, u64), AnyhowRpcError>;

	#[method(name = "update_range_order")]
	async fn update_range_order(
		&self,
		base_asset: RpcAsset,
		pair_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
	) -> Result<Vec<RangeOrderReturn>, AnyhowRpcError>;

	#[method(name = "set_range_order")]
	async fn set_range_order(
		&self,
		base_asset: RpcAsset,
		pair_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
	) -> Result<Vec<RangeOrderReturn>, AnyhowRpcError>;

	#[method(name = "update_limit_order")]
	async fn update_limit_order(
		&self,
		sell_asset: RpcAsset,
		buy_asset: RpcAsset,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
	) -> Result<Vec<LimitOrderReturn>, AnyhowRpcError>;

	#[method(name = "set_limit_order")]
	async fn set_limit_order(
		&self,
		sell_asset: RpcAsset,
		buy_asset: RpcAsset,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount: NumberOrHex,
	) -> Result<Vec<LimitOrderReturn>, AnyhowRpcError>;

	#[method(name = "asset_balances")]
	async fn asset_balances(
		&self,
	) -> Result<BTreeMap<ForeignChain, Vec<AssetBalance>>, AnyhowRpcError>;

	#[method(name = "get_open_swap_channels")]
	async fn get_open_swap_channels(&self) -> Result<OpenSwapChannels, AnyhowRpcError>;
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

#[async_trait]
impl RpcServer for RpcServerImpl {
	/// Returns a deposit address
	async fn request_liquidity_deposit_address(
		&self,
		asset: RpcAsset,
	) -> Result<String, AnyhowRpcError> {
		Ok(self
			.api
			.lp_api()
			.request_liquidity_deposit_address(asset.try_into()?)
			.await
			.map(|address| address.to_string())?)
	}

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> Result<Hash, AnyhowRpcError> {
		let ewa_address = chainflip_api::clean_foreign_chain_address(chain, address)?;
		Ok(self.api.lp_api().register_liquidity_refund_address(ewa_address).await?)
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: RpcAsset,
		destination_address: &str,
	) -> Result<(ForeignChain, u64), AnyhowRpcError> {
		let asset: Asset = asset.try_into()?;

		let destination_address =
			chainflip_api::clean_foreign_chain_address(asset.into(), destination_address)?;

		Ok(self
			.api
			.lp_api()
			.withdraw_asset(try_parse_number_or_hex(amount)?, asset, destination_address)
			.await?)
	}

	/// Returns a list of all assets and their free balance in json format
	async fn asset_balances(
		&self,
	) -> Result<BTreeMap<ForeignChain, Vec<AssetBalance>>, AnyhowRpcError> {
		let mut balances = BTreeMap::<_, Vec<_>>::new();
		for (asset, balance) in self.api.query_api().get_balances(None).await? {
			balances
				.entry(ForeignChain::from(asset))
				.or_default()
				.push(AssetBalance { asset, balance });
		}
		Ok(balances)
	}

	async fn update_range_order(
		&self,
		base_asset: RpcAsset,
		pair_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
	) -> Result<Vec<RangeOrderReturn>, AnyhowRpcError> {
		Ok(self
			.api
			.lp_api()
			.update_range_order(
				base_asset.try_into()?,
				pair_asset.try_into()?,
				id.try_into()?,
				tick_range,
				size_change.try_map(|size| size.try_into())?,
			)
			.await?)
	}

	async fn set_range_order(
		&self,
		base_asset: RpcAsset,
		pair_asset: RpcAsset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
	) -> Result<Vec<RangeOrderReturn>, AnyhowRpcError> {
		Ok(self
			.api
			.lp_api()
			.set_range_order(
				base_asset.try_into()?,
				pair_asset.try_into()?,
				id.try_into()?,
				tick_range,
				size.try_into()?,
			)
			.await?)
	}

	async fn update_limit_order(
		&self,
		sell_asset: RpcAsset,
		buy_asset: RpcAsset,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
	) -> Result<Vec<LimitOrderReturn>, AnyhowRpcError> {
		Ok(self
			.api
			.lp_api()
			.update_limit_order(
				sell_asset.try_into()?,
				buy_asset.try_into()?,
				id.try_into()?,
				tick,
				amount_change.try_map(try_parse_number_or_hex)?,
			)
			.await?)
	}

	async fn set_limit_order(
		&self,
		sell_asset: RpcAsset,
		buy_asset: RpcAsset,
		id: OrderIdJson,
		tick: Option<Tick>,
		sell_amount: NumberOrHex,
	) -> Result<Vec<LimitOrderReturn>, AnyhowRpcError> {
		Ok(self
			.api
			.lp_api()
			.set_limit_order(
				sell_asset.try_into()?,
				buy_asset.try_into()?,
				id.try_into()?,
				tick,
				try_parse_number_or_hex(sell_amount)?,
			)
			.await?)
	}

	/// Returns the tx hash that the account role was set
	async fn register_account(&self) -> Result<Hash, AnyhowRpcError> {
		Ok(self
			.api
			.operator_api()
			.register_account_role(AccountRole::LiquidityProvider)
			.await?)
	}

	async fn get_open_swap_channels(&self) -> Result<OpenSwapChannels, AnyhowRpcError> {
		let api = self.api.query_api();

		let (ethereum, bitcoin, polkadot) = tokio::try_join!(
			api.get_open_swap_channels::<Ethereum>(None),
			api.get_open_swap_channels::<Bitcoin>(None),
			api.get_open_swap_channels::<Polkadot>(None),
		)?;
		Ok(OpenSwapChannels { ethereum, bitcoin, polkadot })
	}
}

#[derive(Parser, Debug, Clone, Default)]
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
			let server = server.start(RpcServerImpl::new(scope, opts).await?.into_rpc());

			log::info!("🎙 Server is listening on {server_addr}.");

			server.stopped().await;
			Ok(())
		}
		.boxed()
	})
	.await
}
