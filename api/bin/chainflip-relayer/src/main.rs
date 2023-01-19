use anyhow::anyhow;
use chainflip_api::{
	self,
	primitives::{AccountRole, Asset, BasisPoints, ForeignChain, ForeignChainAddress},
	settings::StateChain,
};
use clap::Parser;
use jsonrpsee::{
	core::{async_trait, Error},
	proc_macros::rpc,
	server::ServerBuilder,
};
use std::path::PathBuf;

#[rpc(server, client, namespace = "relayer")]
pub trait Rpc {
	#[method(name = "registerAccount")]
	async fn register_account(&self) -> Result<(), Error>;

	#[method(name = "newSwapIngressAddress")]
	async fn request_swap_ingress_address(
		&self,
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: String,
		relayer_commission_bps: BasisPoints,
	) -> Result<String, Error>;
}

pub struct RpcServerImpl {
	state_chain_settings: StateChain,
}

impl RpcServerImpl {
	pub fn new(RelayerOptions { ws_endpoint, signing_key_file, .. }: RelayerOptions) -> Self {
		Self { state_chain_settings: StateChain { ws_endpoint, signing_key_file } }
	}
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	async fn register_account(&self) -> Result<(), Error> {
		Ok(chainflip_api::register_account_role(AccountRole::Relayer, &self.state_chain_settings)
			.await?)
	}
	async fn request_swap_ingress_address(
		&self,
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: String,
		relayer_commission_bps: BasisPoints,
	) -> Result<String, Error> {
		let clean_egress_address = match ForeignChain::from(egress_asset) {
			ForeignChain::Ethereum => ForeignChainAddress::Eth(
				utilities::clean_eth_address(&egress_address).map_err(|e| anyhow!(e))?,
			),
			ForeignChain::Polkadot => ForeignChainAddress::Dot(
				utilities::clean_dot_address(&egress_address).map_err(|e| anyhow!(e))?,
			),
		};
		Ok(chainflip_api::register_swap_intent(
			&self.state_chain_settings,
			ingress_asset,
			egress_asset,
			clean_egress_address,
			relayer_commission_bps,
		)
		.await
		.map(|address| ["0x", &hex::encode(address.as_ref())].concat())
		.map_err(|e| anyhow!("{}:{}", e, e.root_cause()))?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
pub struct RelayerOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the relayer will listen for connections. Use 0 to assing a random port."
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
	let opts = RelayerOptions::parse();
	chainflip_api::use_chainflip_account_id_encoding();
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
	let server_addr = server.local_addr()?;
	let server = server.start(RpcServerImpl::new(opts).into_rpc())?;

	println!("🎙 Server is listening on {server_addr}.");

	server.stopped().await;

	Ok(())
}
