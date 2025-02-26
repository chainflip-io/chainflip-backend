use cf_utilities::{
	health::{self, HealthCheckOptions},
	task_scope::{task_scope, Scope},
};
use chainflip_api::{
	self,
	primitives::{
		state_chain_runtime::runtime_apis::{
			ChainAccounts, ChannelActionType, TransactionScreeningEvents,
		},
		AccountRole, Affiliates, Asset, BasisPoints, CcmChannelMetadata, DcaParameters,
	},
	settings::StateChain,
	AccountId32, AddressString, BlockUpdate, BrokerApi, ChainApi, DepositMonitorApi, OperatorApi,
	RefundParameters, SignedExtrinsicApi, StateChainApi, SwapDepositAddress, TransactionInId,
	WithdrawFeesDetail,
};
use clap::Parser;
use custom_rpc::CustomApiClient;
use futures::{FutureExt, StreamExt};
use jsonrpsee::{
	core::{async_trait, ClientError, SubscriptionResult},
	proc_macros::rpc,
	server::ServerBuilder,
	types::{ErrorCode, ErrorObject, ErrorObjectOwned},
	PendingSubscriptionSink,
};
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize)]
pub enum GetOpenDepositChannelsQuery {
	All,
	Mine,
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

	#[method(name = "mark_transaction_for_rejection", aliases = ["broker_MarkTransactionForRejection"])]
	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()>;

	#[method(name = "get_open_deposit_channels", aliases = ["broker_getOpenDepositChannels"])]
	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts>;

	#[method(name = "all_open_deposit_channels", aliases = ["broker_allOpenDepositChannels"])]
	async fn all_open_deposit_channels(
		&self,
	) -> RpcResult<Vec<(AccountId32, ChannelActionType, ChainAccounts)>>;

	#[subscription(name = "subscribe_transaction_screening_events", item = BlockUpdate<TransactionScreeningEvents>)]
	async fn subscribe_transaction_screening_events(&self) -> SubscriptionResult;
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
				affiliate_fees.unwrap_or_default(),
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

	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()> {
		self.api
			.deposit_monitor_api()
			.mark_transaction_for_rejection(tx_id)
			.await
			.map_err(BrokerApiError::Other)?;
		Ok(())
	}

	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts> {
		let account_id = match query {
			GetOpenDepositChannelsQuery::All => None,
			GetOpenDepositChannelsQuery::Mine => Some(self.api.state_chain_client.account_id()),
		};

		self.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_get_open_deposit_channels(account_id, None)
			.await
			.map_err(BrokerApiError::ClientError)
	}

	async fn all_open_deposit_channels(
		&self,
	) -> RpcResult<Vec<(AccountId32, ChannelActionType, ChainAccounts)>> {
		self.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_all_open_deposit_channels(None)
			.await
			.map_err(BrokerApiError::ClientError)
	}

	async fn subscribe_transaction_screening_events(
		&self,
		pending_sink: PendingSubscriptionSink,
	) -> SubscriptionResult {
		let state_chain_client = self.api.state_chain_client.clone();
		let sink = pending_sink.accept().await.map_err(|e| e.to_string())?;
		tokio::spawn(async move {
			// Note we construct the subscription here rather than relying on the custom-rpc
			// subscription. This is because we want to use finalized blocks.
			// TODO: allow custom rpc subscriptions to use finalized blocks.
			let mut finalized_block_stream = state_chain_client.finalized_block_stream().await;
			while let Some(block) = finalized_block_stream.next().await {
				match state_chain_client
					.base_rpc_client
					.raw_rpc_client
					.cf_get_transaction_screening_events(Some(block.hash))
					.await
				{
					Ok(events) => {
						let block_update = BlockUpdate {
							block_hash: block.hash,
							block_number: block.number,
							data: events,
						};

						if sink
							.send(sc_rpc::utils::to_sub_message(&sink, &block_update))
							.await
							.is_err()
						{
							break;
						}
					},
					Err(_) => break,
				}
			}
		});
		Ok(())
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
