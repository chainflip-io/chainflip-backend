use anyhow::anyhow;
use cf_utilities::{
	health::{self, HealthCheckOptions},
	task_scope::{task_scope, Scope},
};
use chainflip_api::{
	self,
	primitives::{
		state_chain_runtime::runtime_apis::{ChainAccounts, TransactionScreeningEvents},
		AffiliateShortId,
	},
	settings::StateChain,
	AccountId32, BaseRpcApi, BlockUpdate, BrokerApi, ChainflipApi, ChannelId, CustomApiClient,
	DepositMonitorApi, StateChainApi, TransactionInId,
};
use clap::Parser;
use futures::{stream, FutureExt, StreamExt};
use jsonrpsee::core::ClientError;
use jsonrpsee_flatten::{
	core::{async_trait, SubscriptionResult},
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

mod api;
use api::{schema::SchemaApi, *};
use api_json_schema::*;

#[derive(thiserror::Error, Debug)]
pub enum BrokerApiError {
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
	#[error(transparent)]
	ClientError(#[from] ClientError),
	#[error(transparent)]
	Anyhow(#[from] anyhow::Error),
	#[error(transparent)]
	Never(#[from] api_json_schema::Never),
	#[error("The Broker Api does not have a State Chain connection configured.")]
	NoConnection,
}

type RpcResult<T> = Result<T, BrokerApiError>;

impl From<BrokerApiError> for ErrorObjectOwned {
	fn from(error: BrokerApiError) -> Self {
		match error {
			BrokerApiError::ErrorObject(error) => error,
			BrokerApiError::ClientError(error) => match error {
				ClientError::Call(obj) => ErrorObject::owned(obj.code(), obj.message(), obj.data()),
				internal => {
					log::error!("Internal rpc client error: {internal:?}");
					ErrorObject::owned(
						ErrorCode::InternalError.code(),
						"Internal rpc client error",
						None::<()>,
					)
				},
			},
			other => ErrorObjectOwned::owned(
				ErrorCode::ServerError(0xcf).code(),
				other.to_string(),
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
	#[method(
		name = "register_account",
		aliases = ["broker_registerAccount"],
		param_kind = map
	)]
	async fn register_account(
		&self,
		#[argument(flatten)] request: EndpointRequest<register_account::Endpoint>,
	) -> RpcResult<EndpointResponse<register_account::Endpoint>>;

	#[method(
		name = "request_swap_deposit_address",
		aliases = ["broker_requestSwapDepositAddress"],
		param_kind = map
	)]
	async fn request_swap_deposit_address(
		&self,
		#[argument(flatten)] request: EndpointRequest<request_swap_deposit_address::Endpoint>,
	) -> RpcResult<EndpointResponse<request_swap_deposit_address::Endpoint>>;

	#[method(
		name = "withdraw_fees",
		aliases = ["broker_withdrawFees"],
		param_kind = map
	)]
	async fn withdraw_fees(
		&self,
		#[argument(flatten)] request: EndpointRequest<withdraw_fees::Endpoint>,
	) -> RpcResult<EndpointResponse<withdraw_fees::Endpoint>>;

	#[method(
		name = "request_swap_parameter_encoding",
		aliases = ["broker_requestSwapParameterEncoding"],
		param_kind = map,
		deny_array
	)]
	async fn request_swap_parameter_encoding(
		&self,
		#[argument(flatten)] request: EndpointRequest<request_swap_parameter_encoding::Endpoint>,
	) -> RpcResult<EndpointResponse<request_swap_parameter_encoding::Endpoint>>;

	// Not migrated to json_schema yet
	#[method(name = "mark_transaction_for_rejection", aliases = ["broker_MarkTransactionForRejection"])]
	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()>;

	#[method(name = "get_open_deposit_channels", aliases = ["broker_getOpenDepositChannels"])]
	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts>;

	#[subscription(name = "subscribe_transaction_screening_events", item = BlockUpdate<TransactionScreeningEvents>)]
	async fn subscribe_transaction_screening_events(&self) -> SubscriptionResult;

	#[method(name = "open_private_btc_channel", aliases = ["broker_openPrivateBtcChannel"])]
	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "close_private_btc_channel", aliases = ["broker_closePrivateBtcChannel"])]
	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "register_affiliate", aliases = ["broker_registerAffiliate"])]
	async fn register_affiliate(
		&self,
		affiliate_id: AccountId32,
		short_id: Option<AffiliateShortId>,
	) -> RpcResult<AffiliateShortId>;

	#[method(name = "get_affiliates", aliases = ["broker_getAffiliates"])]
	async fn get_affiliates(&self) -> RpcResult<Vec<(AffiliateShortId, AccountId32)>>;

	#[method(name = "schema", param_kind = map)]
	async fn schema(
		&self,
		#[argument(flatten)] request: EndpointRequest<schema::Endpoint>,
	) -> RpcResult<EndpointResponse<schema::Endpoint>>;
}

#[async_trait]
impl RpcServer for RpcServerImpl {
	async fn register_account(
		&self,
		request: EndpointRequest<register_account::Endpoint>,
	) -> RpcResult<EndpointResponse<register_account::Endpoint>> {
		Ok(api_json_schema::respond::<_, api::register_account::Endpoint>(
			self.chainflip_api()?,
			request,
		)
		.await?)
	}

	async fn request_swap_deposit_address(
		&self,
		request: EndpointRequest<request_swap_deposit_address::Endpoint>,
	) -> RpcResult<EndpointResponse<request_swap_deposit_address::Endpoint>> {
		Ok(api_json_schema::respond::<_, request_swap_deposit_address::Endpoint>(
			self.chainflip_api()?,
			request,
		)
		.await?)
	}

	async fn withdraw_fees(
		&self,
		request: EndpointRequest<withdraw_fees::Endpoint>,
	) -> RpcResult<EndpointResponse<withdraw_fees::Endpoint>> {
		Ok(api_json_schema::respond::<_, withdraw_fees::Endpoint>(self.chainflip_api()?, request)
			.await?)
	}

	async fn request_swap_parameter_encoding(
		&self,
		request: EndpointRequest<request_swap_parameter_encoding::Endpoint>,
	) -> RpcResult<EndpointResponse<request_swap_parameter_encoding::Endpoint>> {
		Ok(api_json_schema::respond::<_, request_swap_parameter_encoding::Endpoint>(
			self.chainflip_api()?,
			request,
		)
		.await?)
	}

	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()> {
		self.chainflip_api()?
			.deposit_monitor_api()
			.mark_transaction_for_rejection(tx_id)
			.await
			.map_err(BrokerApiError::Anyhow)?;
		Ok(())
	}

	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts> {
		let account_id = match query {
			GetOpenDepositChannelsQuery::All => None,
			GetOpenDepositChannelsQuery::Mine => Some(self.chainflip_api()?.account_id()),
		};

		self.chainflip_api()?
			.base_rpc_api()
			.raw_rpc_client()
			.cf_get_open_deposit_channels(account_id, None)
			.await
			.map_err(BrokerApiError::ClientError)
	}

	async fn subscribe_transaction_screening_events(
		&self,
		pending_sink: PendingSubscriptionSink,
	) -> SubscriptionResult {
		// pipe results through from custom-rpc subscription
		if let Ok(api) = self.chainflip_api() {
			match api
				.base_rpc_api()
				.raw_rpc_client()
				.cf_subscribe_transaction_screening_events()
				.await
			{
				Ok(subscription) => {
					let stream = stream::unfold(subscription, move |mut sub| async move {
						match sub.next().await {
							Some(Ok(block_update)) => Some((block_update, sub)),
							_ => None,
						}
					})
					.boxed();

					tokio::spawn(async move {
						// SAFETY: the types are in fact the same, but the compiler can't tell that.
						let pending_sink = unsafe {
							core::mem::transmute::<
								jsonrpsee_flatten::PendingSubscriptionSink,
								jsonrpsee::PendingSubscriptionSink,
							>(pending_sink)
						};
						sc_rpc::utils::pipe_from_stream(pending_sink, stream).await;
					});
				},
				Err(e) => {
					pending_sink.reject(BrokerApiError::ClientError(e)).await;
				},
			}
		} else {
			pending_sink.reject(BrokerApiError::NoConnection).await;
		}

		Ok(())
	}

	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId> {
		Ok(self.chainflip_api()?.broker_api().open_private_btc_channel().await?)
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		Ok(self.chainflip_api()?.broker_api().close_private_btc_channel().await?)
	}

	async fn register_affiliate(
		&self,
		affiliate_id: AccountId32,
		short_id: Option<AffiliateShortId>,
	) -> RpcResult<AffiliateShortId> {
		Ok(self
			.chainflip_api()?
			.broker_api()
			.register_affiliate(affiliate_id.clone(), short_id)
			.await?)
	}

	async fn get_affiliates(&self) -> RpcResult<Vec<(AffiliateShortId, AccountId32)>> {
		let api = self.chainflip_api()?;
		Ok(api
			.base_rpc_api()
			.raw_rpc_client()
			.cf_get_affiliates(api.account_id(), None)
			.await?)
	}

	async fn schema(
		&self,
		request: EndpointRequest<schema::Endpoint>,
	) -> RpcResult<EndpointResponse<schema::Endpoint>> {
		Ok(api_json_schema::respond(SchemaApi, request).await?)
	}
}

struct RpcServerImpl {
	api: Option<StateChainApi>,
}

impl RpcServerImpl {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		BrokerOptions { connection, .. }: BrokerOptions,
	) -> Result<Self, anyhow::Error> {
		Ok(Self {
			api: if let Some(ConnectionOptions { ws_endpoint, signing_key_file }) = connection {
				Some(
					StateChainApi::connect(scope, StateChain { ws_endpoint, signing_key_file })
						.await?,
				)
			} else {
				None
			},
		})
	}

	pub fn chainflip_api(&self) -> RpcResult<ApiWrapper<impl ChainflipApi>> {
		Ok(ApiWrapper { api: self.api.as_ref().ok_or(BrokerApiError::NoConnection).cloned()? })
	}
}

struct MockServerImpl;

#[async_trait]
impl RpcServer for MockServerImpl {
	async fn register_account(
		&self,
		request: EndpointRequest<register_account::Endpoint>,
	) -> RpcResult<EndpointResponse<register_account::Endpoint>> {
		Ok(api_json_schema::respond::<_, register_account::Endpoint>(MockApi, request).await?)
	}
	async fn request_swap_deposit_address(
		&self,
		request: EndpointRequest<request_swap_deposit_address::Endpoint>,
	) -> RpcResult<EndpointResponse<request_swap_deposit_address::Endpoint>> {
		Ok(api_json_schema::respond::<_, request_swap_deposit_address::Endpoint>(MockApi, request)
			.await?)
	}
	async fn request_swap_parameter_encoding(
		&self,
		request: EndpointRequest<request_swap_parameter_encoding::Endpoint>,
	) -> RpcResult<EndpointResponse<request_swap_parameter_encoding::Endpoint>> {
		Ok(api_json_schema::respond::<_, request_swap_parameter_encoding::Endpoint>(
			MockApi, request,
		)
		.await?)
	}
	async fn withdraw_fees(
		&self,
		request: EndpointRequest<withdraw_fees::Endpoint>,
	) -> RpcResult<EndpointResponse<withdraw_fees::Endpoint>> {
		Ok(api_json_schema::respond::<_, withdraw_fees::Endpoint>(MockApi, request).await?)
	}

	async fn mark_transaction_for_rejection(&self, _tx_id: TransactionInId) -> RpcResult<()> {
		Err(BrokerApiError::Anyhow(anyhow!("Example not implemented.")))
	}

	async fn get_open_deposit_channels(
		&self,
		_query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts> {
		Err(BrokerApiError::Anyhow(anyhow!("Example not implemented.")))
	}

	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId> {
		Err(BrokerApiError::Anyhow(anyhow!("Example not implemented.")))
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		Err(BrokerApiError::Anyhow(anyhow!("Example not implemented.")))
	}

	async fn register_affiliate(
		&self,
		_affiliate_id: AccountId32,
		_short_id: Option<AffiliateShortId>,
	) -> RpcResult<AffiliateShortId> {
		Err(BrokerApiError::Anyhow(anyhow!("Example not implemented.")))
	}

	async fn get_affiliates(&self) -> RpcResult<Vec<(AffiliateShortId, AccountId32)>> {
		Err(BrokerApiError::Anyhow(anyhow!("Example not implemented.")))
	}

	async fn subscribe_transaction_screening_events(
		&self,
		_subscription_sink: jsonrpsee_flatten::PendingSubscriptionSink,
	) -> SubscriptionResult {
		Err("Example not implemented.".into())
	}

	async fn schema(
		&self,
		request: EndpointRequest<schema::Endpoint>,
	) -> RpcResult<EndpointResponse<schema::Endpoint>> {
		Ok(api_json_schema::respond(SchemaApi, request).await?)
	}
}

#[derive(Parser, Debug, Clone, Copy)]

enum SubCommand {
	Schema {
		#[clap(long = "inline-defs", default_value = "false")]
		inline_defs: bool,
	},
	Mock,
}

#[derive(clap::Args, Debug, Clone, Default)]
struct ConnectionOptions {
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

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"))]
struct BrokerOptions {
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
	#[clap(flatten)]
	pub connection: Option<ConnectionOptions>,
	#[clap(flatten)]
	pub health_check: HealthCheckOptions,
	#[clap(subcommand)]
	pub subcommand: Option<SubCommand>,
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
			match opts.subcommand {
				Some(SubCommand::Schema { inline_defs }) => {
					let schemas = api_json_schema::respond(
						SchemaApi,
						schema::SchemaRequest { inline_defs, ..Default::default() },
					)
					.await?;
					println!("{}", serde_json::to_string(&schemas)?);
					Ok(())
				},
				subcommand => {
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

					let server = if let Some(SubCommand::Mock) = subcommand {
						server.start(MockServerImpl.into_rpc())
					} else {
						server.start(RpcServerImpl::new(scope, opts).await?.into_rpc())
					};

					log::info!("🎙 Server is listening on {server_addr}.");

					// notify healthcheck completed
					has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

					server.stopped().await;

					Ok(())
				},
			}
		}
		.boxed()
	})
	.await
}
