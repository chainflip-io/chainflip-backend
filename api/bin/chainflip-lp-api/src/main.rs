use anyhow::anyhow;
use cf_utilities::{task_scope::task_scope, try_parse_number_or_hex};
use chainflip_api::{
	self,
	lp::{self, BuyOrSellOrder, MintRangeOrderReturn, Tick},
	primitives::{
		chains::{Bitcoin, Ethereum, Polkadot},
		AccountRole, Asset, ForeignChain,
	},
	queries::SwapChannelInfo,
	settings::StateChain,
	AccountId32,
};
use clap::Parser;
use futures::FutureExt;
use jsonrpsee::{
	core::{async_trait, Error, __reexports::serde_json},
	proc_macros::rpc,
	server::ServerBuilder,
};
use pallet_cf_pools::Pool;
use sp_rpc::number::NumberOrHex;
use std::{collections::BTreeMap, ops::Range, path::PathBuf};

/// Contains RPC interface types that differ from internal types.
pub mod rpc_types {
	use chainflip_api::{lp, primitives::AssetAmount};
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

	/// Range Orders can be specified in terms of either asset amounts or pool liquidity.
	///
	/// If `AssetAmounts` is specified, the order requires desired and minimum amounts of the assets
	/// pairs. This will attempt a mint of up to `desired` amounts of the assets, but will not mint
	/// less than `minimum` amounts.
	#[derive(Serialize, Deserialize)]
	pub enum RangeOrderSize {
		AssetAmounts { desired: AssetAmounts, minimum: AssetAmounts },
		PoolLiquidity(NumberOrHex),
	}

	impl TryFrom<RangeOrderSize> for lp::RangeOrderSize {
		type Error = <u128 as TryFrom<NumberOrHex>>::Error;

		fn try_from(value: RangeOrderSize) -> Result<Self, Self::Error> {
			Ok(match value {
				RangeOrderSize::AssetAmounts { desired, minimum } => Self::AssetAmounts {
					desired: desired.try_into()?,
					minimum: minimum.try_into()?,
				},
				RangeOrderSize::PoolLiquidity(liquidity) => Self::Liquidity(liquidity.try_into()?),
			})
		}
	}
}

#[rpc(server, client, namespace = "lp")]
pub trait Rpc {
	#[method(name = "registerAccount")]
	async fn register_account(&self) -> Result<String, Error>;

	#[method(name = "liquidityDeposit")]
	async fn request_liquidity_deposit_address(&self, asset: Asset) -> Result<String, Error>;

	#[method(name = "registerEmergencyWithdrawalAddress")]
	async fn register_emergency_withdrawal_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> Result<String, Error>;

	#[method(name = "withdrawAsset")]
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: &str,
	) -> Result<String, Error>;

	#[method(name = "mintRangeOrder")]
	async fn mint_range_order(
		&self,
		asset: Asset,
		lower_tick: Tick,
		upper_tick: Tick,
		order_size: rpc_types::RangeOrderSize,
	) -> Result<MintRangeOrderReturn, Error>;

	#[method(name = "burnRangeOrder")]
	async fn burn_range_order(
		&self,
		asset: Asset,
		lower_tick: Tick,
		upper_tick: Tick,
		amount: NumberOrHex,
	) -> Result<String, Error>;

	#[method(name = "mintLimitOrder")]
	async fn mint_limit_order(
		&self,
		asset: Asset,
		order: BuyOrSellOrder,
		price: Tick,
		amount: NumberOrHex,
	) -> Result<String, Error>;

	#[method(name = "burnLimitOrder")]
	async fn burn_limit_order(
		&self,
		asset: Asset,
		order: BuyOrSellOrder,
		price: Tick,
		amount: NumberOrHex,
	) -> Result<String, Error>;

	#[method(name = "assetBalances")]
	async fn asset_balances(&self) -> Result<String, Error>;

	#[method(name = "getRangeOrders")]
	async fn get_range_orders(&self) -> Result<String, Error>;

	#[method(name = "getOpenSwapChannels")]
	async fn get_open_swap_channels(&self) -> Result<Vec<SwapChannelInfo>, Error>;

	#[method(name = "getPools")]
	async fn get_pools(&self) -> Result<BTreeMap<Asset, Pool<AccountId32>>, Error>;

	#[method(name = "getPool")]
	async fn get_pool(&self, asset: Asset) -> Result<BTreeMap<Asset, Pool<AccountId32>>, Error>;
}
pub struct RpcServerImpl {
	state_chain_settings: StateChain,
}

impl RpcServerImpl {
	pub fn new(LPOptions { ws_endpoint, signing_key_file, .. }: LPOptions) -> Self {
		Self { state_chain_settings: StateChain { ws_endpoint, signing_key_file } }
	}
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	/// Returns a deposit address
	async fn request_liquidity_deposit_address(&self, asset: Asset) -> Result<String, Error> {
		lp::request_liquidity_deposit_address(&self.state_chain_settings, asset)
			.await
			.map(|address| address.to_string())
			.map_err(|e| Error::Custom(e.to_string()))
	}

	async fn register_emergency_withdrawal_address(
		&self,
		chain: ForeignChain,
		address: &str,
	) -> Result<String, Error> {
		let ewa_address = chainflip_api::clean_foreign_chain_address(chain, address)
			.map_err(|e| Error::Custom(e.to_string()))?;
		lp::register_emergency_withdrawal_address(&self.state_chain_settings, ewa_address)
			.await
			.map(|tx_hash| tx_hash.to_string())
			.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: &str,
	) -> Result<String, Error> {
		let destination_address =
			chainflip_api::clean_foreign_chain_address(asset.into(), destination_address)
				.map_err(|e| Error::Custom(e.to_string()))?;

		lp::withdraw_asset(
			&self.state_chain_settings,
			try_parse_number_or_hex(amount)?,
			asset,
			destination_address,
		)
		.await
		.map(|(_, id)| id.to_string())
		.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Returns a list of all assets and their free balance in json format
	async fn asset_balances(&self) -> Result<String, Error> {
		lp::get_balances(&self.state_chain_settings)
			.await
			.map(|balances| {
				serde_json::to_string(&balances).expect("Should output balances as json")
			})
			.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Returns a list of all assets and their range order positions in json format
	async fn get_range_orders(&self) -> Result<String, Error> {
		lp::get_range_orders(&self.state_chain_settings)
			.await
			.map(|positions| {
				serde_json::to_string(&positions).expect("Should output range orders as json")
			})
			.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Creates or adds liquidity to a range order.
	/// Returns the assets debited and fees harvested.
	async fn mint_range_order(
		&self,
		asset: Asset,
		start: Tick,
		end: Tick,
		order_size: rpc_types::RangeOrderSize,
	) -> Result<MintRangeOrderReturn, Error> {
		if start >= end {
			return Err(Error::Custom("Invalid tick range".to_string()))
		}

		lp::mint_range_order(
			&self.state_chain_settings,
			asset,
			Range { start, end },
			order_size.try_into().map_err(|_| anyhow!("Invalid order size."))?,
		)
		.await
		.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Removes liquidity from a range order.
	/// Returns the assets returned and fees harvested.
	async fn burn_range_order(
		&self,
		asset: Asset,
		start: Tick,
		end: Tick,
		amount: NumberOrHex,
	) -> Result<String, Error> {
		if start >= end {
			return Err(Error::Custom("Invalid tick range".to_string()))
		}

		lp::burn_range_order(
			&self.state_chain_settings,
			asset,
			Range { start, end },
			try_parse_number_or_hex(amount)?,
		)
		.await
		.map(|data| serde_json::to_string(&data).expect("should serialize return struct"))
		.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Creates or adds liquidity to a limit order.
	/// Returns the assets debited, fees harvested and swapped liquidity.
	async fn mint_limit_order(
		&self,
		asset: Asset,
		order: BuyOrSellOrder,
		price: Tick,
		amount: NumberOrHex,
	) -> Result<String, Error> {
		lp::mint_limit_order(
			&self.state_chain_settings,
			asset,
			order,
			price,
			try_parse_number_or_hex(amount)?,
		)
		.await
		.map(|data| serde_json::to_string(&data).expect("should serialize return struct"))
		.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Removes liquidity from a limit order.
	/// Returns the assets credited, fees harvested and swapped liquidity.
	async fn burn_limit_order(
		&self,
		asset: Asset,
		order: BuyOrSellOrder,
		price: Tick,
		amount: NumberOrHex,
	) -> Result<String, Error> {
		lp::burn_limit_order(
			&self.state_chain_settings,
			asset,
			order,
			price,
			try_parse_number_or_hex(amount)?,
		)
		.await
		.map(|data| serde_json::to_string(&data).expect("should serialize return struct"))
		.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Returns the tx hash that the account role was set
	async fn register_account(&self) -> Result<String, Error> {
		chainflip_api::register_account_role(
			AccountRole::LiquidityProvider,
			&self.state_chain_settings,
		)
		.await
		.map(|tx_hash| format!("{tx_hash:#x}"))
		.map_err(|e| Error::Custom(e.to_string()))
	}

	async fn get_open_swap_channels(&self) -> Result<Vec<SwapChannelInfo>, Error> {
		task_scope(|scope| {
			async move {
				let (client, latest_hash) =
					chainflip_api::queries::connect(scope, &self.state_chain_settings).await?;

				tokio::try_join!(
					chainflip_api::queries::get_open_swap_channels::<Ethereum>(
						client.clone(),
						latest_hash,
					),
					chainflip_api::queries::get_open_swap_channels::<Bitcoin>(
						client.clone(),
						latest_hash,
					),
					chainflip_api::queries::get_open_swap_channels::<Polkadot>(client, latest_hash),
				)
				.map(|(eth_result, btc_result, dot_result)| {
					[eth_result, btc_result, dot_result].into_iter().flatten().collect::<Vec<_>>()
				})
			}
			.boxed()
		})
		.await
		.map_err(|e| Error::Custom(e.to_string()))
	}

	async fn get_pool(&self, asset: Asset) -> Result<BTreeMap<Asset, Pool<AccountId32>>, Error> {
		task_scope(|scope| {
			async move {
				let (client, latest_hash) =
					chainflip_api::queries::connect(scope, &self.state_chain_settings).await?;
				chainflip_api::queries::get_pools(client, latest_hash, Some(asset)).await
			}
			.boxed()
		})
		.await
		.map_err(|e| Error::Custom(e.to_string()))
	}

	async fn get_pools(&self) -> Result<BTreeMap<Asset, Pool<AccountId32>>, Error> {
		task_scope(|scope| {
			async move {
				let (client, latest_hash) =
					chainflip_api::queries::connect(scope, &self.state_chain_settings).await?;
				chainflip_api::queries::get_pools(client, latest_hash, None).await
			}
			.boxed()
		})
		.await
		.map_err(|e| Error::Custom(e.to_string()))
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

	let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
	let server_addr = server.local_addr()?;
	let server = server.start(RpcServerImpl::new(opts).into_rpc())?;

	println!("🎙 Server is listening on {server_addr}.");

	server.stopped().await;

	Ok(())
}
