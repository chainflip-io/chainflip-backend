use chainflip_api::{
	self,
	lp::{self, Liquidity, Tick},
	primitives::{AccountRole, Asset, AssetAmount},
	settings::StateChain,
};
use clap::Parser;
use jsonrpsee::{
	core::{async_trait, Error, __reexports::serde_json},
	proc_macros::rpc,
	server::ServerBuilder,
};
use std::{ops::Range, path::PathBuf};

#[rpc(server, client, namespace = "lp")]
pub trait Rpc {
	#[method(name = "registerAccount")]
	async fn register_account(&self) -> Result<String, Error>;

	#[method(name = "liquidityDeposit")]
	async fn liquidity_deposit(&self, asset: Asset) -> Result<String, Error>;

	#[method(name = "withdrawAsset")]
	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		egress_address: &str,
	) -> Result<String, Error>;

	#[method(name = "mintRangeOrder")]
	async fn mint_range_order(
		&self,
		asset: Asset,
		lower_tick: Tick,
		upper_tick: Tick,
		amount: Liquidity,
	) -> Result<String, Error>;

	#[method(name = "burnRangeOrder")]
	async fn burn_range_order(
		&self,
		asset: Asset,
		lower_tick: Tick,
		upper_tick: Tick,
		amount: Liquidity,
	) -> Result<String, Error>;

	#[method(name = "tokenBalances")]
	async fn token_balances(&self) -> Result<String, Error>;

	#[method(name = "getRangeOrders")]
	async fn get_range_orders(&self) -> Result<String, Error>;
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
	/// Returns an ingress address
	async fn liquidity_deposit(&self, asset: Asset) -> Result<String, Error> {
		lp::liquidity_deposit(&self.state_chain_settings, asset)
			.await
			.map(|address| address.to_string())
			.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		egress_address: &str,
	) -> Result<String, Error> {
		if amount == 0 {
			return Err(Error::Custom("Invalid amount".to_string()))
		}

		let egress_address = chainflip_api::clean_foreign_chain_address(
			asset,
			egress_address,
			&self.state_chain_settings,
		)
		.await
		.map_err(|e| Error::Custom(e.to_string()))?;

		lp::withdraw_asset(&self.state_chain_settings, amount, asset, egress_address)
			.await
			.map(|(_, id)| id.to_string())
			.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Returns a list of all assets and their free balance in json format
	async fn token_balances(&self) -> Result<String, Error> {
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
		amount: Liquidity,
	) -> Result<String, Error> {
		if start >= end {
			return Err(Error::Custom("Invalid tick range".to_string()))
		}

		lp::mint_range_order(&self.state_chain_settings, asset, Range { start, end }, amount)
			.await
			.map(|data| serde_json::to_string(&data).expect("should serialize return struct"))
			.map_err(|e| Error::Custom(e.to_string()))
	}

	/// Removes liquidity from a rage order.
	/// Returns the assets returned and fees harvested.
	async fn burn_range_order(
		&self,
		asset: Asset,
		start: Tick,
		end: Tick,
		amount: Liquidity,
	) -> Result<String, Error> {
		if start >= end {
			return Err(Error::Custom("Invalid tick range".to_string()))
		}

		lp::burn_range_order(&self.state_chain_settings, asset, Range { start, end }, amount)
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
