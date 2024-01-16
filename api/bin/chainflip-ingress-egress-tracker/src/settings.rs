use chainflip_engine::settings::{
	insert_command_line_option, CfSettings, HttpBasicAuthEndpoint, WsHttpEndpoints,
};
use clap::Parser;
use config::{Config, ConfigBuilder, ConfigError, Environment, Map, Source, Value};
use serde::Deserialize;
use std::{collections::HashMap, env};

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
	#[clap(long = "redis_url")]
	redis_url: Option<String>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct DepositTrackerSettings {
	pub eth: WsHttpEndpoints,
	pub dot: WsHttpEndpoints,
	pub state_chain_ws_endpoint: String,
	pub btc: HttpBasicAuthEndpoint,
	pub redis_url: String,
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
			.set_default("btc.basic_auth_password", "flip")?
			.set_default("redis_url", "http://127.0.0.1:6380")
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
		insert_command_line_option(&mut map, "redis_url", &self.redis_url);

		Ok(map)
	}
}
