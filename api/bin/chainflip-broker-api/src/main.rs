use cf_utilities::{
	health::{self, HealthCheckOptions},
	rpc::NumberOrHex,
	task_scope::{task_scope, Scope},
	try_parse_number_or_hex,
};
use chainflip_api::{
	self,
	primitives::{AccountRole, Affiliates, Asset, BasisPoints, CcmChannelMetadata, DcaParameters},
	settings::StateChain,
	AccountId32, AddressString, OperatorApi, RefundParameters, StateChainApi, SwapDepositAddress,
	WithdrawFeesDetail,
};
use clap::Parser;
use custom_rpc::VaultSwapDetailsHumanreadable;
use futures::FutureExt;
use jsonrpsee::{
	core::{async_trait, ClientError},
	proc_macros::rpc,
	server::ServerBuilder,
	types::{ErrorCode, ErrorObject, ErrorObjectOwned},
};
use std::{
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc},
};
use tracing::log;

#[derive(thiserror::Error, Debug)]
pub enum BrokerApiError {
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
	#[error(transparent)]
	ClientError(#[from] jsonrpsee::core::ClientError),
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

type RpcResult<T> = Result<T, BrokerApiError>;

impl From<BrokerApiError> for ErrorObjectOwned {
	fn from(error: BrokerApiError) -> Self {
		match error {
			BrokerApiError::ErrorObject(error) => error,
			BrokerApiError::ClientError(error) => match error {
				ClientError::Call(obj) => obj,
				internal => {
					log::error!("Internal rpc client error: {internal:?}");
					ErrorObject::owned(
						ErrorCode::InternalError.code(),
						"Internal rpc client error",
						None::<()>,
					)
				},
			},
			BrokerApiError::Other(error) => jsonrpsee::types::error::ErrorObjectOwned::owned(
				ErrorCode::ServerError(0xcf).code(),
				error.to_string(),
				None::<()>,
			),
		}
	}
}

#[rpc(server, client, namespace = "broker")]
pub trait Rpc {
	#[method(name = "register_account", aliases = ["broker_registerAccount"])]
	async fn register_account(&self) -> RpcResult<String>;

	#[method(name = "request_swap_deposit_address", aliases = ["broker_requestSwapDepositAddress"])]
	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		refund_parameters: Option<RefundParameters>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<SwapDepositAddress>;

	#[method(name = "withdraw_fees", aliases = ["broker_withdrawFees"])]
	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail>;

	#[method(name = "request_swap_parameter_encoding", aliases = ["broker_requestSwapParameterEncoding"])]
	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		min_output_amount: NumberOrHex,
		retry_duration: u32,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetailsHumanreadable>;
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
	async fn register_account(&self) -> RpcResult<String> {
		Ok(self
			.api
			.operator_api()
			.register_account_role(AccountRole::Broker)
			.await
			.map(|tx_hash| format!("{tx_hash:#x}"))?)
	}

	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		refund_parameters: Option<RefundParameters>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<SwapDepositAddress> {
		Ok(self
			.api
			.broker_api()
			.request_swap_deposit_address(
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				refund_parameters,
				dca_parameters,
			)
			.await?)
	}

	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail> {
		Ok(self.api.broker_api().withdraw_fees(asset, destination_address).await?)
	}

	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		min_output_amount: NumberOrHex,
		retry_duration: u32,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetailsHumanreadable> {
		Ok(self
			.api
			.broker_api()
			.request_swap_parameter_encoding(
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				try_parse_number_or_hex(min_output_amount)?,
				retry_duration,
				boost_fee,
				affiliate_fees,
				dca_parameters,
			)
			.await?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"))]
pub struct BrokerOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the broker will listen for connections. Use 0 to assign a random port."
	)]
	pub port: u16,
	#[clap(
		long = "max_connections",
		default_value = "100",
		help = "The maximum number of concurrent websocket connections to accept."
	)]
	pub max_connections: u32,
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
	#[clap(flatten)]
	pub health_check: HealthCheckOptions,
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
			// initialize healthcheck endpoint
			let has_completed_initialising = Arc::new(AtomicBool::new(false));
			health::start_if_configured(
				scope,
				&opts.health_check,
				has_completed_initialising.clone(),
			)
			.await?;

			let server = ServerBuilder::default()
				.max_connections(opts.max_connections)
				.build(format!("0.0.0.0:{}", opts.port))
				.await?;
			let server_addr = server.local_addr()?;
			let server = server.start(RpcServerImpl::new(scope, opts).await?.into_rpc());

			log::info!("🎙 Server is listening on {server_addr}.");

			// notify healthcheck completed
			has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

			server.stopped().await;

			Ok(())
		}
		.boxed()
	})
	.await
}
