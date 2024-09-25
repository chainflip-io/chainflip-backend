use cf_utilities::{
	health::{self, HealthCheckOptions},
	task_scope::{task_scope, Scope},
};
use chainflip_api::{
	self,
	primitives::{AccountRole, Affiliates, Asset, BasisPoints, CcmChannelMetadata, DcaParameters},
	settings::StateChain,
	AccountId32, AddressString, BrokerApi, OperatorApi, RefundParameters, StateChainApi,
	SwapDepositAddress, WithdrawFeesDetail,
};
use clap::Parser;
use custom_rpc::to_rpc_error;
use futures::FutureExt;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	server::ServerBuilder,
};
use std::{
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc},
};
use tracing::log;

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
			.map(|tx_hash| format!("{tx_hash:#x}"))
			.map_err(to_rpc_error)?)
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
				affiliate_fees.unwrap_or_default(),
				refund_parameters,
				dca_parameters,
			)
			.await
			.map_err(to_rpc_error)?)
	}

	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail> {
		Ok(self
			.api
			.broker_api()
			.withdraw_fees(asset, destination_address)
			.await
			.map_err(to_rpc_error)?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), short_flag = 'v')]
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
				.await
				.map_err(to_rpc_error)?;
			let server_addr = server.local_addr()?;
			let server = server
				.start(RpcServerImpl::new(scope, opts).await.map_err(to_rpc_error)?.into_rpc());

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
