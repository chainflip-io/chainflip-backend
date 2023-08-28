use cf_primitives::AssetAmount;
use cf_utilities::{
	task_scope::{task_scope, Scope},
	try_parse_number_or_hex,
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
use futures::FutureExt;
use jsonrpsee::{
	core::{async_trait, Error},
	proc_macros::rpc,
	server::ServerBuilder,
};
use pallet_cf_pools::{IncreaseOrDecrease, OrderId, RangeOrderSize};
use rpc_types::OpenSwapChannels;
use sp_rpc::number::NumberOrHex;
use std::{collections::BTreeMap, ops::Range, path::PathBuf};
use tracing::log;

/// Contains RPC interface types that differ from internal types.
pub mod rpc_types {
	use super::*;
	use chainflip_api::{lp, primitives::AssetAmount, queries::SwapChannelInfo};
	use serde::{Deserialize, Serialize};
	use sp_rpc::number::NumberOrHex;

	#[derive(Serialize, Deserialize)]
	pub struct AssetAmounts {
		/// The amount of the unstable asset.
		///
		/// This is side `zero` in the AMM.
		unstable: NumberOrHex,
		/// The amount of the stable asset (USDC).
		///
		/// This is side `one` in the AMM.
		stable: NumberOrHex,
	}

	impl TryFrom<AssetAmounts> for lp::SideMap<AssetAmount> {
		type Error = <u128 as TryFrom<NumberOrHex>>::Error;

		fn try_from(value: AssetAmounts) -> Result<Self, Self::Error> {
			Ok(lp::SideMap::from_array([value.unstable.try_into()?, value.stable.try_into()?]))
		}
	}

	#[derive(Serialize, Deserialize)]
	pub struct RangeOrder {
		pub lower_tick: i32,
		pub upper_tick: i32,
		pub liquidity: u128,
	}

	#[derive(Serialize, Deserialize)]
	pub struct OpenSwapChannels {
		pub ethereum: Vec<SwapChannelInfo<Ethereum>>,
		pub bitcoin: Vec<SwapChannelInfo<Bitcoin>>,
		pub polkadot: Vec<SwapChannelInfo<Polkadot>>,
	}
}

#[rpc(server, client, namespace = "lp")]
pub trait Rpc {
	#[method(name = "registerAccount")]
	async fn register_account(&self) -> Result<Hash, Error>;

	#[method(name = "liquidityDeposit")]
	async fn request_liquidity_deposit_address(&self, asset: Asset) -> Result<String, Error>;

	#[method(name = "registerEmergencyWithdrawalAddress")]
	async fn register_emergency_withdrawal_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> Result<Hash, Error>;

	#[method(name = "withdrawAsset")]
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: &str,
	) -> Result<(ForeignChain, u64), Error>;

	#[method(name = "updateRangeOrder")]
	async fn update_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		tick_range: Option<Range<Tick>>,
		increase_or_decrease: IncreaseOrDecrease,
		size: RangeOrderSize,
	) -> Result<Vec<RangeOrderReturn>, Error>;

	#[method(name = "setRangeOrder")]
	async fn set_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
	) -> Result<Vec<RangeOrderReturn>, Error>;

	#[method(name = "updateLimitOrder")]
	async fn update_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		tick: Option<Tick>,
		increase_or_decrease: IncreaseOrDecrease,
		amount: AssetAmount,
	) -> Result<Vec<LimitOrderReturn>, Error>;

	#[method(name = "setLimitOrder")]
	async fn set_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		tick: Option<Tick>,
		amount: AssetAmount,
	) -> Result<Vec<LimitOrderReturn>, Error>;

	#[method(name = "assetBalances")]
	async fn asset_balances(&self) -> Result<BTreeMap<Asset, u128>, Error>;

	#[method(name = "getOpenSwapChannels")]
	async fn get_open_swap_channels(&self) -> Result<OpenSwapChannels, Error>;
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
	async fn request_liquidity_deposit_address(&self, asset: Asset) -> Result<String, Error> {
		self.api
			.lp_api()
			.request_liquidity_deposit_address(asset)
			.await
			.map(|address| address.to_string())
			.map_err(|e| Error::Custom(e.to_string()))
	}

	async fn register_emergency_withdrawal_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> Result<Hash, Error> {
		let ewa_address = chainflip_api::clean_foreign_chain_address(chain, address)
			.map_err(|e| Error::Custom(e.to_string()))?;
		Ok(self.api.lp_api().register_emergency_withdrawal_address(ewa_address).await?)
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: &str,
	) -> Result<(ForeignChain, u64), Error> {
		let destination_address =
			chainflip_api::clean_foreign_chain_address(asset.into(), destination_address)?;

		Ok(self
			.api
			.lp_api()
			.withdraw_asset(try_parse_number_or_hex(amount)?, asset, destination_address)
			.await?)
	}

	/// Returns a list of all assets and their free balance in json format
	async fn asset_balances(&self) -> Result<BTreeMap<Asset, u128>, Error> {
		Ok(self.api.query_api().get_balances(None).await?)
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		tick_range: Option<Range<Tick>>,
		increase_or_decrease: IncreaseOrDecrease,
		size: RangeOrderSize,
	) -> Result<Vec<RangeOrderReturn>, Error> {
		Ok(self
			.api
			.lp_api()
			.update_range_order(base_asset, pair_asset, id, tick_range, increase_or_decrease, size)
			.await?)
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		pair_asset: Asset,
		id: OrderId,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
	) -> Result<Vec<RangeOrderReturn>, Error> {
		Ok(self
			.api
			.lp_api()
			.set_range_order(base_asset, pair_asset, id, tick_range, size)
			.await?)
	}

	async fn update_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		tick: Option<Tick>,
		increase_or_decrease: IncreaseOrDecrease,
		amount: AssetAmount,
	) -> Result<Vec<LimitOrderReturn>, Error> {
		Ok(self
			.api
			.lp_api()
			.update_limit_order(sell_asset, buy_asset, id, tick, increase_or_decrease, amount)
			.await?)
	}

	async fn set_limit_order(
		&self,
		sell_asset: Asset,
		buy_asset: Asset,
		id: OrderId,
		tick: Option<Tick>,
		sell_amount: AssetAmount,
	) -> Result<Vec<LimitOrderReturn>, Error> {
		Ok(self
			.api
			.lp_api()
			.set_limit_order(sell_asset, buy_asset, id, tick, sell_amount)
			.await?)
	}

	/// Returns the tx hash that the account role was set
	async fn register_account(&self) -> Result<Hash, Error> {
		Ok(self
			.api
			.operator_api()
			.register_account_role(AccountRole::LiquidityProvider)
			.await?)
	}

	async fn get_open_swap_channels(&self) -> Result<OpenSwapChannels, Error> {
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
			let server = server.start(RpcServerImpl::new(scope, opts).await?.into_rpc())?;

			log::info!("🎙 Server is listening on {server_addr}.");

			server.stopped().await;
			Ok(())
		}
		.boxed()
	})
	.await
}
