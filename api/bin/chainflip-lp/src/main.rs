use anyhow::anyhow;
use chainflip_api::{
	self, lp,
	primitives::{AccountRole, AmmRange, Asset, AssetAmount, ForeignChainAddress, Liquidity, Tick},
	settings::StateChain,
};
use clap::Parser;
use jsonrpsee::{
	core::{async_trait, Error, __reexports::serde_json},
	proc_macros::rpc,
	server::ServerBuilder,
};
use std::path::PathBuf;
use tracing::warn;
use utilities::{clean_dot_address, clean_eth_address};

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

	#[method(name = "mintPosition")]
	async fn mint_position(
		&self,
		asset: Asset,
		lower_tick: Tick,
		upper_tick: Tick,
		amount: Liquidity,
	) -> Result<String, Error>;

	#[method(name = "burnPosition")]
	async fn burn_position(
		&self,
		asset: Asset,
		lower_tick: Tick,
		upper_tick: Tick,
		amount: Liquidity,
	) -> Result<String, Error>;

	#[method(name = "tokenBalances")]
	async fn token_balances(&self) -> Result<String, Error>;

	#[method(name = "positions")]
	async fn positions(&self) -> Result<String, Error>;
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
		Ok(lp::liquidity_deposit(&self.state_chain_settings, asset)
			.await
			.map(|address| ["0x", &hex::encode(address.as_ref())].concat())
			.map_err(|e| anyhow!("{e}"))?)
	}

	/// Returns an egress id
	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		egress_address: &str,
	) -> Result<String, Error> {
		if amount == 0 {
			Err(anyhow!("Invalid amount"))?
		}

		// Sanitize the address
		let egress_address = match asset {
			Asset::Eth => clean_eth_address(egress_address)
				.map(ForeignChainAddress::from)
				.map_err(|e| anyhow!("Invalid egress_address: {e}")),
			Asset::Dot => clean_dot_address(egress_address)
				.map(ForeignChainAddress::from)
				.map_err(|e| anyhow!("Invalid egress_address: {e}")),
			_ => return Err(Error::Custom(format!("{asset:?} not supported"))),
		}?;

		Ok(lp::withdraw_asset(&self.state_chain_settings, amount, asset, egress_address)
			.await
			.map(|(_, id)| format!("{id}"))
			.map_err(|e| anyhow!("{e}"))?)
	}

	/// Returns a list of all assets and their free balance in json format
	async fn token_balances(&self) -> Result<String, Error> {
		Ok(lp::get_balances(&self.state_chain_settings)
			.await
			.map(|balances| {
				serde_json::to_string(&balances).expect("Should output balances as json")
			})
			.map_err(|e| anyhow!("{e}"))?)
	}

	/// Returns a list of all assets and their positions in json format
	async fn positions(&self) -> Result<String, Error> {
		Ok(lp::get_positions(&self.state_chain_settings)
			.await
			.map(|positions| {
				serde_json::to_string(&positions).expect("Should output positions as json")
			})
			.map_err(|e| anyhow!("{e}"))?)
	}

	/// Creates or adds liquidity to a position.
	/// Returns the assets debited and fees harvested.
	async fn mint_position(
		&self,
		asset: Asset,
		lower: Tick,
		upper: Tick,
		amount: Liquidity,
	) -> Result<String, Error> {
		if lower >= upper {
			Err(anyhow!("Invalid tick range"))?
		}

		Ok(lp::mint_position(&self.state_chain_settings, asset, AmmRange { lower, upper }, amount)
			.await
			.map(|data| serde_json::to_string(&data).expect("should serialize return struct"))
			.map_err(|e| anyhow!("{e}"))?)
	}

	/// Removes liquidity from a position.
	/// Returns the assets returned and fees harvested.
	async fn burn_position(
		&self,
		asset: Asset,
		lower: Tick,
		upper: Tick,
		amount: Liquidity,
	) -> Result<String, Error> {
		if lower >= upper {
			Err(anyhow!("Invalid tick range"))?
		}

		Ok(lp::burn_position(&self.state_chain_settings, asset, AmmRange { lower, upper }, amount)
			.await
			.map(|data| serde_json::to_string(&data).expect("should serialize return struct"))
			.map_err(|e| anyhow!("{e}"))?)
	}

	/// Returns the tx hash that the account role was set
	async fn register_account(&self) -> Result<String, Error> {
		Ok(chainflip_api::register_account_role(
			AccountRole::LiquidityProvider,
			&self.state_chain_settings,
		)
		.await
		.map(|tx_hash| format!("{tx_hash:#x}"))
		.map_err(|e| anyhow!("{e}"))?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
pub struct LPOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the relayer will listen for connections. Use 0 to assign a random port."
	)]
	pub port: u16,
	#[clap(
		long = "state_chain.ws_endpoint",
		default_value = "ws://localhost:9944",
		help = "The state chain node's rpc endpoint."
	)]
	pub ws_endpoint: String,
	#[clap(
		long = "state_chain.signing_key_file",
		default_value = "/etc/chainflip/keys/signing_key_file",
		help = "A path to a file that contains the relayer's secret key for signing extrinsics."
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

	if !opts.signing_key_file.exists() {
		warn!("No signing_key_file found at {}", opts.signing_key_file.to_string_lossy())
	}

	let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
	let server_addr = server.local_addr()?;
	let server = server.start(RpcServerImpl::new(opts).into_rpc())?;

	println!("🎙 Server is listening on {server_addr}.");

	server.stopped().await;

	Ok(())
}
