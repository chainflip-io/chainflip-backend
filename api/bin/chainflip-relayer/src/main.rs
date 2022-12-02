use anyhow::anyhow;
use chainflip_api::{
	self,
	primitives::{Asset, ForeignChain, ForeignChainAddress},
	settings::StateChain,
};
use jsonrpsee::{
	core::{async_trait, Error},
	proc_macros::rpc,
	server::{ServerBuilder, ServerHandle},
};
use std::net::SocketAddr;

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
		Ok(chainflip_api::register_swap_intent(
			&self.state_chain_settings,
			ingress_asset,
			egress_asset,
			egress_address,
			relayer_commission_bps.unwrap_or_default(),
		)
		.await?)
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let (server, server_addr) = run_server().await?;

	println!("🎙 Server is listening on ws://{}.", server_addr);

	server.stopped().await;

	Ok(())
}

async fn run_server() -> anyhow::Result<(ServerHandle, SocketAddr)> {
	let server = ServerBuilder::default().build("0.0.0.0:0").await?;

	let addr = server.local_addr()?;
	let handle = server.start(
		RpcServerImpl {
			state_chain_settings: StateChain {
				ws_endpoint: "wss://localhost:9944".into(),
				signing_key_file: "/etc/chainflip/keys/signing_key_file".into(),
			},
		}
		.into_rpc(),
	)?;

	Ok((handle, addr))
}
