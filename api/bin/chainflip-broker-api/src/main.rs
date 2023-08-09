use anyhow::anyhow;
use cf_utilities::task_scope::{task_scope, Scope};
use chainflip_api::{
	self, clean_foreign_chain_address,
	primitives::{AccountRole, Asset, BasisPoints, BlockNumber, CcmChannelMetadata, ChannelId},
	settings::StateChain,
	BrokerApi, OperatorApi, StateChainApi,
};
use clap::Parser;
use futures::FutureExt;
use hex::FromHexError;
use jsonrpsee::{
	core::{async_trait, Error},
	proc_macros::rpc,
	server::ServerBuilder,
};
use serde::{Deserialize, Serialize};
use sp_rpc::number::NumberOrHex;
use std::path::PathBuf;
use tracing::log;

/// The response type expected by the broker api.
///
/// Note that changing this struct is a breaking change to the api.
#[derive(Serialize, Deserialize)]
pub struct BrokerSwapDepositAddress {
	pub address: String,
	pub expiry_block: BlockNumber,
	pub issued_block: BlockNumber,
	pub channel_id: ChannelId,
}

impl From<chainflip_api::SwapDepositAddress> for BrokerSwapDepositAddress {
	fn from(value: chainflip_api::SwapDepositAddress) -> Self {
		Self {
			address: value.address,
			expiry_block: value.expiry_block,
			issued_block: value.issued_block,
			channel_id: value.channel_id,
		}
	}
}

#[derive(Serialize, Deserialize)]
pub struct BrokerCcmChannelMetadata {
	gas_budget: NumberOrHex,
	message: String,
	cf_parameters: Option<String>,
}

fn parse_hex_bytes(string: &str) -> Result<Vec<u8>, FromHexError> {
	hex::decode(string.strip_prefix("0x").unwrap_or(string))
}

#[cfg(test)]
mod test {
	use super::*;
	use cf_utilities::assert_err;

	#[test]
	fn test_decoding() {
		assert_eq!(parse_hex_bytes("0x00").unwrap(), vec![0]);
		assert_eq!(parse_hex_bytes("cf").unwrap(), vec![0xcf]);
		assert_eq!(
			parse_hex_bytes("0x00112233445566778899aabbccddeeff").unwrap(),
			vec![
				0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
				0xee, 0xff
			]
		);
		assert_eq!(parse_hex_bytes("").unwrap(), b"");
		assert_err!(parse_hex_bytes("abc"));
	}
}

impl TryInto<CcmChannelMetadata> for BrokerCcmChannelMetadata {
	type Error = anyhow::Error;

	fn try_into(self) -> Result<CcmChannelMetadata, Self::Error> {
		let gas_budget = self
			.gas_budget
			.try_into()
			.map_err(|_| anyhow!("Failed to parse {:?} as gas budget", self.gas_budget))?;
		let message =
			parse_hex_bytes(&self.message).map_err(|e| anyhow!("Failed to parse message: {e}"))?;

		let cf_parameters = self
			.cf_parameters
			.map(|parameters| parse_hex_bytes(&parameters))
			.transpose()
			.map_err(|e| anyhow!("Failed to parse cf parameters: {e}"))?
			.unwrap_or_default();

		Ok(CcmChannelMetadata { gas_budget, message, cf_parameters })
	}
}

#[rpc(server, client, namespace = "broker")]
pub trait Rpc {
	#[method(name = "registerAccount")]
	async fn register_account(&self) -> Result<String, Error>;

	#[method(name = "requestSwapDepositAddress")]
	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: String,
		broker_commission_bps: BasisPoints,
		channel_metadata: Option<BrokerCcmChannelMetadata>,
	) -> Result<BrokerSwapDepositAddress, Error>;
}

pub struct RpcServerImpl {
	api: StateChainApi,
}

impl RpcServerImpl {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		BrokerOptions { ws_endpoint, signing_key_file, .. }: BrokerOptions,
	) -> Result<Self, anyhow::Error> {
		Ok(Self {
			api: StateChainApi::connect(scope, StateChain { ws_endpoint, signing_key_file })
				.await?,
		})
	}
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	async fn register_account(&self) -> Result<String, Error> {
		self.api
			.operator_api()
			.register_account_role(AccountRole::Broker)
			.await
			.map(|tx_hash| format!("{tx_hash:#x}"))
			.map_err(Into::into)
	}

	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: String,
		broker_commission_bps: BasisPoints,
		channel_metadata: Option<BrokerCcmChannelMetadata>,
	) -> Result<BrokerSwapDepositAddress, Error> {
		let channel_metadata = channel_metadata.map(TryInto::try_into).transpose()?;

		self.api
			.broker_api()
			.request_swap_deposit_address(
				source_asset,
				destination_asset,
				clean_foreign_chain_address(destination_asset.into(), &destination_address)?,
				broker_commission_bps,
				channel_metadata,
			)
			.await
			.map(BrokerSwapDepositAddress::from)
			.map_err(Into::into)
	}
}

#[derive(Parser, Debug, Clone, Default)]
pub struct BrokerOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the broker will listen for connections. Use 0 to assign a random port."
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
		help = "A path to a file that contains the broker's secret key for signing extrinsics."
	)]
	pub signing_key_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let opts = BrokerOptions::parse();
	chainflip_api::use_chainflip_account_id_encoding();
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

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
