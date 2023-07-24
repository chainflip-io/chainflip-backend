use anyhow::anyhow;
use chainflip_api::{
	self, clean_foreign_chain_address,
	primitives::{
		AccountRole, Asset, BasisPoints, BlockNumber, CcmDepositMetadata, ChannelId,
		ForeignChainAddress,
	},
	settings::StateChain,
};
use clap::Parser;
use hex::FromHexError;
use jsonrpsee::{
	core::{async_trait, Error},
	proc_macros::rpc,
	server::ServerBuilder,
};
use serde::{Deserialize, Serialize};
use sp_rpc::number::NumberOrHex;
use std::path::PathBuf;

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
pub struct BrokerCcmDepositMetadata {
	gas_budget: NumberOrHex,
	message: String,
	cf_parameters: Option<String>,
}

fn parse_hex_bytes(string: &str) -> Result<Vec<u8>, FromHexError> {
	hex::decode(string.strip_prefix("0x").unwrap_or(string))
}

#[cfg(test)]
mod test {
	use utilities::assert_err;

	use super::*;

	#[test]
	fn test_decoding() {
		assert_eq!(parse_hex_bytes("0x00").unwrap(), b"00");
		assert_eq!(parse_hex_bytes("").unwrap(), b"");
		assert_err!(parse_hex_bytes("abc"));
	}
}

impl TryInto<CcmDepositMetadata> for BrokerCcmDepositMetadata {
	type Error = anyhow::Error;

	fn try_into(self) -> Result<CcmDepositMetadata, Self::Error> {
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

		Ok(CcmDepositMetadata {
			gas_budget,
			message,
			cf_parameters,
			// This is temporary until we remove the deprecated field.
			source_address: ForeignChainAddress::Eth(*b"deprecateddeprecated"),
		})
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
		message_metadata: Option<BrokerCcmDepositMetadata>,
	) -> Result<BrokerSwapDepositAddress, Error>;
}

pub struct RpcServerImpl {
	state_chain_settings: StateChain,
}

impl RpcServerImpl {
	pub fn new(BrokerOptions { ws_endpoint, signing_key_file, .. }: BrokerOptions) -> Self {
		Self { state_chain_settings: StateChain { ws_endpoint, signing_key_file } }
	}
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	async fn register_account(&self) -> Result<String, Error> {
		Ok(chainflip_api::register_account_role(AccountRole::Broker, &self.state_chain_settings)
			.await
			.map(|tx_hash| format!("{tx_hash:#x}"))?)
	}
	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: String,
		broker_commission_bps: BasisPoints,
		message_metadata: Option<BrokerCcmDepositMetadata>,
	) -> Result<BrokerSwapDepositAddress, Error> {
		let message_metadata = message_metadata.map(TryInto::try_into).transpose()?;

		Ok(chainflip_api::request_swap_deposit_address(
			&self.state_chain_settings,
			source_asset,
			destination_asset,
			clean_foreign_chain_address(destination_asset.into(), &destination_address)?,
			broker_commission_bps,
			message_metadata,
		)
		.await?)
		.map(BrokerSwapDepositAddress::from)
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

	let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
	let server_addr = server.local_addr()?;
	let server = server.start(RpcServerImpl::new(opts).into_rpc())?;

	println!("🎙 Server is listening on {server_addr}.");

	server.stopped().await;

	Ok(())
}
