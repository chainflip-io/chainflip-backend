use chainflip_engine::settings::{
	insert_command_line_option, CfSettings, HttpBasicAuthEndpoint, WsHttpEndpoints,
};
use clap::Parser;
use config::{Config, ConfigBuilder, ConfigError, Environment, Map, Source, Value};
use futures::FutureExt;
use jsonrpsee::{core::Error, server::ServerBuilder, RpcModule};
use serde::Deserialize;
use std::{collections::HashMap, env, net::SocketAddr};
use tracing::log;
use utilities::task_scope;

mod witnessing;

#[derive(Clone, Deserialize, Debug)]
pub struct DepositTrackerSettings {
	eth: WsHttpEndpoints,
	dot: WsHttpEndpoints,
	state_chain_ws_endpoint: String,
	btc: HttpBasicAuthEndpoint,
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), version_short = 'v')]
pub struct TrackerOptions {
	#[clap(long = "eth.rpc.ws_endpoint")]
	eth_ws_endpoint: Option<String>,
	#[clap(long = "eth.rpc.http_endpoint")]
	eth_http_endpoint: Option<String>,
	#[clap(long = "dot.rpc.ws_endpoint")]
	dot_ws_endpoint: Option<String>,
	#[clap(long = "dot.rpc.http_endpoint")]
	dot_http_endpoint: Option<String>,
	#[clap(long = "state_chain.ws_endpoint")]
	state_chain_ws_endpoint: Option<String>,
	#[clap(long = "btc.rpc.http_endpoint")]
	btc_endpoint: Option<String>,
	#[clap(long = "btc.rpc.basic_auth_user")]
	btc_username: Option<String>,
	#[clap(long = "btc.rpc.basic_auth_password")]
	btc_password: Option<String>,
}

impl CfSettings for DepositTrackerSettings {
	type CommandLineOptions = TrackerOptions;

	fn load_settings_from_all_sources(
		config_root: String,
		_settings_dir: &str,
		opts: Self::CommandLineOptions,
	) -> Result<Self, ConfigError> {
		Self::set_defaults(Config::builder(), &config_root)?
			.add_source(Environment::default().separator("__"))
			.add_source(opts)
			.build()?
			.try_deserialize()
	}

	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
		_config_root: &str,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		// These defaults are for a localnet setup
		config_builder
			.set_default("eth.ws_endpoint", "ws://localhost:8546")?
			.set_default("eth.http_endpoint", "http://localhost:8545")?
			.set_default("dot.ws_endpoint", "ws://localhost:9947")?
			.set_default("dot.http_endpoint", "http://localhost:9947")?
			.set_default("state_chain_ws_endpoint", "ws://localhost:9944")?
			.set_default("btc.http_endpoint", "http://127.0.0.1:8332")?
			.set_default("btc.basic_auth_user", "flip")?
			.set_default("btc.basic_auth_password", "flip")
	}

	fn validate_settings(
		&mut self,
		_config_root: &std::path::Path,
	) -> anyhow::Result<(), ConfigError> {
		Ok(())
	}
}

impl Source for TrackerOptions {
	fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> std::result::Result<Map<String, Value>, ConfigError> {
		let mut map: HashMap<String, Value> = HashMap::new();

		insert_command_line_option(&mut map, "eth.ws_endpoint", &self.eth_ws_endpoint);
		insert_command_line_option(&mut map, "eth.http_endpoint", &self.eth_http_endpoint);
		insert_command_line_option(&mut map, "dot.ws_endpoint", &self.dot_ws_endpoint);
		insert_command_line_option(&mut map, "dot.http_endpoint", &self.dot_http_endpoint);
		insert_command_line_option(
			&mut map,
			"state_chain_ws_endpoint",
			&self.state_chain_ws_endpoint,
		);
		insert_command_line_option(&mut map, "btc.http_endpoint", &self.btc_endpoint);
		insert_command_line_option(&mut map, "btc.basic_auth_user", &self.btc_username);
		insert_command_line_option(&mut map, "btc.basic_auth_password", &self.btc_password);

		Ok(map)
	}
}

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
) -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");
	let mut module = RpcModule::new(());

	let btc_tracker = witnessing::btc_mempool::start(scope, settings.btc.clone()).await;

	module.register_async_method("status", move |arguments, _context| {
		let btc_tracker = btc_tracker.clone();
		async move {
			arguments.parse::<Vec<String>>().map_err(Error::Call).and_then(|addresses| {
				btc_tracker
					.lookup_transactions(&addresses)
					.map_err(|err| jsonrpsee::core::Error::Custom(err.to_string()))
			})
		}
	})?;

	// Broadcast channel will drop old messages when the buffer is full to
	// avoid "memory leaks" due to slow receivers.
	const EVENT_BUFFER_SIZE: usize = 1024;
	let (witness_sender, _) =
		tokio::sync::broadcast::channel::<state_chain_runtime::RuntimeCall>(EVENT_BUFFER_SIZE);

	witnessing::start(scope, settings, witness_sender.clone()).await?;

	module.register_subscription(
		"subscribe_witnessing",
		"s_witnessing",
		"unsubscribe_witnessing",
		move |_params, mut sink, _context| {
			let mut witness_receiver = witness_sender.subscribe();

			tokio::spawn(async move {
				while let Ok(event) = witness_receiver.recv().await {
					use codec::Encode;
					if let Ok(false) = sink.send(&event.encode()) {
						log::debug!("Subscription is closed");
						break
					}
				}
			});
			Ok(())
		},
	)?;

	scope.spawn(async {
		let server = ServerBuilder::default().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
		let addr = server.local_addr()?;
		log::info!("Listening on http://{}", addr);
		server.start(module)?.stopped().await;
		// If the server stops for some reason, we return
		// error to terminate other tasks and the process.
		Err(anyhow::anyhow!("RPC server stopped"))
	});

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let settings = DepositTrackerSettings::load_settings_from_all_sources(
		// Not using the config root or settings dir.
		"".to_string(),
		"",
		TrackerOptions::parse(),
	)?;

	task_scope::task_scope(|scope| async move { start(scope, settings).await }.boxed()).await
}
