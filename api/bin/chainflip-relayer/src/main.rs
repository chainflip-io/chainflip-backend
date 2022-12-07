use anyhow::anyhow;
use chainflip_api::{
	self,
	primitives::{Asset, ForeignChain, ForeignChainAddress},
	settings::StateChain,
};
use clap::Parser;
use jsonrpsee::{
	core::{async_trait, Error},
	proc_macros::rpc,
	server::ServerBuilder,
	types::error::CallError,
};
use std::path::PathBuf;

#[rpc(server, client, namespace = "relayer")]
pub trait Rpc {
	#[method(name = "requestSwap")]
	async fn request_swap(
		&self,
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: Vec<u8>,
		relayer_commission_bps: Option<u16>,
	) -> Result<ForeignChainAddress, Error>;
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
	async fn request_swap(
		&self,
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: Vec<u8>,
		relayer_commission_bps: Option<u16>,
	) -> Result<ForeignChainAddress, Error> {
		let egress_address = match ForeignChain::from(egress_asset) {
			ForeignChain::Ethereum => ForeignChainAddress::Eth(
				egress_address
					.try_into()
					.map_err(|_| anyhow!("Invalid address format for Ethereum"))?,
			),
			ForeignChain::Polkadot => ForeignChainAddress::Dot(
				egress_address
					.try_into()
					.map_err(|_| anyhow!("Invalid address format for Polkadot"))?,
			),
		};
		chainflip_api::register_swap_intent(
			&self.state_chain_settings,
			ingress_asset,
			egress_asset,
			egress_address,
			relayer_commission_bps.unwrap_or_default(),
		)
		.await
		.map_err(|e| Error::from(CallError::Failed(anyhow!(e.root_cause().to_string()))))
	}
}

#[derive(Parser, Debug, Clone, Default)]
pub struct RelayerOptions {
	#[clap(long = "state_chain.ws_endpoint", default_value = "ws://localhost:9944")]
	pub ws_endpoint: String,
	#[clap(
		long = "state_chain.signing_key_file",
		default_value = "/etc/chainflip/keys/signing_key_file"
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
