use anyhow::anyhow;
use chainflip_api::{
	self,
	primitives::{AccountRole, Asset, ForeignChain, ForeignChainAddress},
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

	#[method(name = "getNewIngressAddress")]
	async fn request_swap_ingress_address(
		&self,
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: String,
		relayer_commission_bps: u16,
	) -> Result<String, Error>;
}

pub struct RpcServerImpl {
	state_chain_settings: StateChain,
}

impl RpcServerImpl {
	pub fn new(RelayerOptions { ws_endpoint, signing_key_file }: RelayerOptions) -> Self {
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
		relayer_commission_bps: u16,
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
		.map(|address| hex::encode(address.as_ref()))?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
pub struct RelayerOptions {
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
	chainflip_api::use_chainflip_account_id_encoding();
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let server = ServerBuilder::default().build("0.0.0.0:0").await?;

	let server_addr = server.local_addr()?;
	let server = server.start(RpcServerImpl::new(RelayerOptions::parse()).into_rpc())?;

	println!("🎙 Server is listening on {}.", server_addr);

	server.stopped().await;

	Ok(())
}
