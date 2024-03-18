use std::{
	collections::HashMap,
	ffi::OsStr,
	fmt,
	net::IpAddr,
	path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use config::{Config, ConfigBuilder, ConfigError, Environment, File, Map, Source, Value};
use serde::{de, Deserialize, Deserializer};

pub use anyhow::Result;
use regex::Regex;
use sp_runtime::DeserializeOwned;
use url::Url;

use clap::Parser;
use utilities::{
	logging::LoggingSettings, metrics::Prometheus, redact_endpoint_secret::SecretUrl, Port,
};

use crate::constants::{CONFIG_ROOT, DEFAULT_CONFIG_ROOT};

pub const DEFAULT_SETTINGS_DIR: &str = "config";

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct P2P {
	#[serde(deserialize_with = "deser_path")]
	pub node_key_file: PathBuf,
	pub ip_address: IpAddr,
	pub port: Port,
	pub allow_local_ip: bool,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct StateChain {
	pub ws_endpoint: String,
	#[serde(deserialize_with = "deser_path")]
	pub signing_key_file: PathBuf,
}

impl StateChain {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		validate_websocket_endpoint(self.ws_endpoint.clone().into())
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct WsHttpEndpoints {
	pub ws_endpoint: SecretUrl,
	pub http_endpoint: SecretUrl,
}

pub trait ValidateSettings {
	fn validate(&self) -> Result<(), ConfigError>;
}

impl ValidateSettings for WsHttpEndpoints {
	/// Ensure the endpoints are valid HTTP and WS endpoints.
	fn validate(&self) -> Result<(), ConfigError> {
		validate_websocket_endpoint(self.ws_endpoint.clone())
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		validate_http_endpoint(self.http_endpoint.clone())
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct NodeContainer<NodeConfig> {
	#[serde(rename = "rpc")]
	pub primary: NodeConfig,
	#[serde(rename = "backup_rpc")]
	pub backup: Option<NodeConfig>,
}

impl<NodeConfig: ValidateSettings> NodeContainer<NodeConfig> {
	pub fn validate(&self) -> Result<(), ConfigError> {
		self.primary.validate()?;
		if let Some(backup) = &self.backup {
			backup.validate()?;
		}
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Evm {
	#[serde(flatten)]
	pub nodes: NodeContainer<WsHttpEndpoints>,
	#[serde(deserialize_with = "deser_path")]
	pub private_key_file: PathBuf,
}

impl Evm {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		self.nodes.validate()
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Dot {
	#[serde(flatten)]
	pub nodes: NodeContainer<WsHttpEndpoints>,
}

impl Dot {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		self.nodes.validate()?;

		// Check that all endpoints have a port number
		let validate_dot_endpoints = |endpoints: &WsHttpEndpoints| -> Result<(), ConfigError> {
			validate_port_exists(&endpoints.ws_endpoint)
				.and_then(|_| validate_port_exists(&endpoints.http_endpoint))
				.map_err(|e| {
					ConfigError::Message(format!(
						"Polkadot node endpoints must include a port number: {e}"
					))
				})
		};
		validate_dot_endpoints(&self.nodes.primary)?;
		if let Some(backup) = &self.nodes.backup {
			validate_dot_endpoints(backup)?;
		}
		Ok(())
	}
}

// Checks that the url has a port number
fn validate_port_exists(url: &SecretUrl) -> Result<()> {
	// NB: We are using regex instead of Url because Url.port() returns None for wss/https urls with
	// default ports.
	let re = Regex::new(r":([0-9]+)").unwrap();
	if re.captures(url.as_ref()).is_none() {
		bail!("No port found in url: {url}");
	}
	Ok(())
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct HttpBasicAuthEndpoint {
	pub http_endpoint: SecretUrl,
	pub basic_auth_user: String,
	pub basic_auth_password: String,
}

impl ValidateSettings for HttpBasicAuthEndpoint {
	/// Ensure the endpoint is a valid HTTP endpoint.
	fn validate(&self) -> Result<(), ConfigError> {
		validate_http_endpoint(self.http_endpoint.clone())
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Btc {
	#[serde(flatten)]
	pub nodes: NodeContainer<HttpBasicAuthEndpoint>,
}

impl Btc {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		self.nodes.validate()
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct HealthCheck {
	pub hostname: String,
	pub port: Port,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Signing {
	#[serde(deserialize_with = "deser_path")]
	pub db_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Settings {
	pub node_p2p: P2P,
	pub state_chain: StateChain,
	// External Chain settings
	pub eth: Evm,
	pub dot: Dot,
	pub btc: Btc,
	pub arb: Evm,

	pub health_check: Option<HealthCheck>,
	pub prometheus: Option<Prometheus>,
	pub signing: Signing,
	pub logging: LoggingSettings,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct StateChainOptions {
	#[clap(long = "state_chain.ws_endpoint")]
	pub state_chain_ws_endpoint: Option<String>,
	#[clap(long = "state_chain.signing_key_file")]
	pub state_chain_signing_key_file: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct EthOptions {
	#[clap(long = "eth.rpc.ws_endpoint")]
	pub eth_ws_endpoint: Option<String>,
	#[clap(long = "eth.rpc.http_endpoint")]
	pub eth_http_endpoint: Option<String>,

	#[clap(long = "eth.backup_rpc.ws_endpoint")]
	pub eth_backup_ws_endpoint: Option<String>,
	#[clap(long = "eth.backup_rpc.http_endpoint")]
	pub eth_backup_http_endpoint: Option<String>,

	#[clap(long = "eth.private_key_file")]
	pub eth_private_key_file: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct DotOptions {
	#[clap(long = "dot.rpc.ws_endpoint")]
	pub dot_ws_endpoint: Option<String>,
	#[clap(long = "dot.rpc.http_endpoint")]
	pub dot_http_endpoint: Option<String>,

	#[clap(long = "dot.backup_rpc.ws_endpoint")]
	pub dot_backup_ws_endpoint: Option<String>,
	#[clap(long = "dot.backup_rpc.http_endpoint")]
	pub dot_backup_http_endpoint: Option<String>,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct BtcOptions {
	#[clap(long = "btc.rpc.http_endpoint")]
	pub btc_http_endpoint: Option<String>,
	#[clap(long = "btc.rpc.basic_auth_user")]
	pub btc_basic_auth_user: Option<String>,
	#[clap(long = "btc.rpc.basic_auth_password")]
	pub btc_basic_auth_password: Option<String>,

	#[clap(long = "btc.backup_rpc.http_endpoint")]
	pub btc_backup_http_endpoint: Option<String>,
	#[clap(long = "btc.backup_rpc.basic_auth_user")]
	pub btc_backup_basic_auth_user: Option<String>,
	#[clap(long = "btc.backup_rpc.basic_auth_password")]
	pub btc_backup_basic_auth_password: Option<String>,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct ArbOptions {
	#[clap(long = "arb.rpc.ws_endpoint")]
	pub arb_ws_endpoint: Option<String>,
	#[clap(long = "arb.rpc.http_endpoint")]
	pub arb_http_endpoint: Option<String>,
	#[clap(long = "arb.private_key_file")]
	pub arb_private_key_file: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct P2POptions {
	#[clap(long = "p2p.node_key_file", parse(from_os_str))]
	node_key_file: Option<PathBuf>,
	#[clap(long = "p2p.ip_address")]
	ip_address: Option<IpAddr>,
	#[clap(long = "p2p.port")]
	p2p_port: Option<Port>,
	#[clap(long = "p2p.allow_local_ip")]
	allow_local_ip: Option<bool>,
}

#[derive(Parser, Debug, Clone)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), version_short = 'v')]
pub struct CommandLineOptions {
	// Misc Options
	#[clap(short = 'c', long = "config-root", env = CONFIG_ROOT, default_value = DEFAULT_CONFIG_ROOT)]
	pub config_root: String,

	#[clap(flatten)]
	pub p2p_opts: P2POptions,

	#[clap(flatten)]
	pub state_chain_opts: StateChainOptions,

	#[clap(flatten)]
	pub eth_opts: EthOptions,

	#[clap(flatten)]
	pub dot_opts: DotOptions,

	#[clap(flatten)]
	pub btc_opts: BtcOptions,

	#[clap(flatten)]
	pub arb_opts: ArbOptions,

	// Health Check Settings
	#[clap(long = "health_check.hostname")]
	pub health_check_hostname: Option<String>,
	#[clap(long = "health_check.port")]
	pub health_check_port: Option<Port>,

	// Prometheus Settings
	#[clap(long = "prometheus.hostname")]
	pub prometheus_hostname: Option<String>,
	#[clap(long = "prometheus.port")]
	pub prometheus_port: Option<Port>,

	// Signing Settings
	#[clap(long = "signing.db_file", parse(from_os_str))]
	pub signing_db_file: Option<PathBuf>,

	// Logging settings
	#[clap(long = "logging.span_lifecycle")]
	pub logging_span_lifecycle: bool,

	#[clap(long = "logging.command_server_port")]
	pub logging_command_server_port: Option<Port>,
}

impl Default for CommandLineOptions {
	fn default() -> Self {
		Self {
			#[cfg(not(test))]
			config_root: DEFAULT_CONFIG_ROOT.to_owned(),
			#[cfg(test)]
			config_root: env!("CF_TEST_CONFIG_ROOT").to_owned(),
			p2p_opts: P2POptions::default(),
			state_chain_opts: StateChainOptions::default(),
			eth_opts: EthOptions::default(),
			dot_opts: DotOptions::default(),
			btc_opts: BtcOptions::default(),
			arb_opts: ArbOptions::default(),
			health_check_hostname: None,
			health_check_port: None,
			prometheus_hostname: None,
			prometheus_port: None,
			signing_db_file: None,
			logging_span_lifecycle: false,
			logging_command_server_port: None,
		}
	}
}

const NODE_P2P_KEY_FILE: &str = "node_p2p.node_key_file";
const NODE_P2P_PORT: &str = "node_p2p.port";
const NODE_P2P_ALLOW_LOCAL_IP: &str = "node_p2p.allow_local_ip";

const STATE_CHAIN_WS_ENDPOINT: &str = "state_chain.ws_endpoint";
const STATE_CHAIN_SIGNING_KEY_FILE: &str = "state_chain.signing_key_file";

const ETH_PRIVATE_KEY_FILE: &str = "eth.private_key_file";
const ARB_PRIVATE_KEY_FILE: &str = "arb.private_key_file";

const SIGNING_DB_FILE: &str = "signing.db_file";

const LOGGING_SPAN_LIFECYCLE: &str = "logging.span_lifecycle";
const LOGGING_COMMAND_SERVER_PORT: &str = "logging.command_server_port";

// We use PathBuf because the value must be Sized, Path is not Sized
fn deser_path<'de, D>(deserializer: D) -> std::result::Result<PathBuf, D::Error>
where
	D: Deserializer<'de>,
{
	struct PathVisitor;

	impl<'de> de::Visitor<'de> for PathVisitor {
		type Value = PathBuf;

		fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
			formatter.write_str("A string containing a path")
		}

		fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
		where
			E: de::Error,
		{
			Ok(PathBuf::from(v))
		}
	}

	// use our visitor to deserialize a `PathBuf`
	deserializer.deserialize_any(PathVisitor)
}

/// Describes behaviour required by a struct to be used for as settings/configuration
pub trait CfSettings
where
	Self: DeserializeOwned,
{
	type CommandLineOptions: Source + Send + Sync + 'static;

	/// Merges settings from a TOML file, environment and provided command line options.
	/// Merge priority is:
	/// 1 - Command line options
	/// 2 - Environment
	/// 3 - TOML file (if found)
	/// 4 - Default value
	fn load_settings_from_all_sources(
		config_root: String,
		// <config_root>/<settings_dir>/Settings.toml is the location of the settings that we'll
		// read.
		settings_dir: &str,
		opts: Self::CommandLineOptions,
	) -> Result<Self, ConfigError> {
		// Set the default settings
		let mut builder = Self::set_defaults(Config::builder(), &config_root)?;

		// If the file does not exist we will try and continue anyway.
		// Because if all of the settings are covered in the environment, cli options and defaults,
		// then we don't need it.
		let settings_file =
			PathBuf::from(config_root.clone()).join(settings_dir).join("Settings.toml");
		let file_present = settings_file.is_file();
		if file_present {
			builder = builder.add_source(File::from(settings_file.clone()));
		} else if config_root != DEFAULT_CONFIG_ROOT {
			// If the user has set a custom base config path but the settings file is missing, then
			// error.
			return Err(ConfigError::Message(format!(
				"File not found: {}",
				settings_file.to_string_lossy()
			)))
		}

		let mut settings: Self = builder
			.add_source(Environment::default().separator("__"))
			.add_source(opts)
			.build()?
			.try_deserialize()
			.map_err(|e| {
				// Add context to the error message if the settings file was missing.
				ConfigError::Message(if file_present {
					e.to_string()
				} else {
					format!("Error in {}: {}", settings_file.to_string_lossy(), e)
				})
			})?;

		settings.validate_settings(&PathBuf::from(&config_root))?;

		Ok(settings)
	}

	/// Set the default values of any settings. These values will be overridden by all other
	/// sources. Any set this way will become optional (If no other source contains the settings, it
	/// will NOT error).
	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
		_config_root: &str,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		// This function is optional, so just pass it through.
		Ok(config_builder)
	}

	/// Validate the formatting of some settings
	fn validate_settings(&mut self, config_root: &Path) -> Result<(), ConfigError>;
}

pub enum PathResolutionExpectation {
	ExistingFile,
	ExistingDir,
}

pub fn resolve_settings_path(
	root: &Path,
	path: &Path,
	expectation: Option<PathResolutionExpectation>,
) -> Result<PathBuf, ConfigError> {
	// Note: if path is already absolute, `join` ignores the `root`.
	let absolute_path = root.join(path);
	match expectation {
		None => Ok(absolute_path),
		Some(expectation) => {
			if !absolute_path.try_exists().map_err(|e| ConfigError::Foreign(Box::new(e)))? {
				Err(ConfigError::Message(format!(
					"Path does not exist: {}",
					absolute_path.to_string_lossy()
				)))
			} else {
				absolute_path
					.canonicalize()
					.map_err(|e| ConfigError::Foreign(Box::new(e)))
					.and_then(|path| {
						if match expectation {
							PathResolutionExpectation::ExistingFile => path.is_file(),
							PathResolutionExpectation::ExistingDir => path.is_dir(),
						} {
							Ok(path)
						} else {
							Err(ConfigError::Message(std::format!(
								"{:?} is not a {}",
								path,
								match expectation {
									PathResolutionExpectation::ExistingFile => "file",
									PathResolutionExpectation::ExistingDir => "path",
								}
							)))
						}
					})
			}
		},
	}
}

impl CfSettings for Settings {
	type CommandLineOptions = CommandLineOptions;

	fn validate_settings(&mut self, config_root: &Path) -> Result<(), ConfigError> {
		self.eth.validate_settings()?;

		self.dot.validate_settings()?;

		self.btc.validate_settings()?;

		self.arb.validate_settings()?;

		self.state_chain.validate_settings()?;

		is_valid_db_path(&self.signing.db_file).map_err(|e| ConfigError::Message(e.to_string()))?;

		self.state_chain.signing_key_file = resolve_settings_path(
			config_root,
			&self.state_chain.signing_key_file,
			Some(PathResolutionExpectation::ExistingFile),
		)?;
		self.eth.private_key_file = resolve_settings_path(
			config_root,
			&self.eth.private_key_file,
			Some(PathResolutionExpectation::ExistingFile),
		)?;
		self.arb.private_key_file = resolve_settings_path(
			config_root,
			&self.arb.private_key_file,
			Some(PathResolutionExpectation::ExistingFile),
		)?;
		self.signing.db_file = resolve_settings_path(config_root, &self.signing.db_file, None)?;
		self.node_p2p.node_key_file = resolve_settings_path(
			config_root,
			&self.node_p2p.node_key_file,
			Some(PathResolutionExpectation::ExistingFile),
		)?;

		Ok(())
	}

	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
		config_root: &str,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		config_builder
			.set_default(NODE_P2P_ALLOW_LOCAL_IP, false)?
			.set_default(LOGGING_SPAN_LIFECYCLE, false)?
			.set_default(LOGGING_COMMAND_SERVER_PORT, 36079)?
			.set_default(
				NODE_P2P_KEY_FILE,
				PathBuf::from(config_root)
					.join("keys/node_key_file")
					.to_str()
					.expect("Invalid node_key_file path"),
			)?
			.set_default(NODE_P2P_PORT, 8078)?
			.set_default(STATE_CHAIN_WS_ENDPOINT, "ws://localhost:9944")?
			.set_default(
				STATE_CHAIN_SIGNING_KEY_FILE,
				PathBuf::from(config_root)
					.join("keys/signing_key_file")
					.to_str()
					.expect("Invalid signing_key_file path"),
			)?
			.set_default(
				ETH_PRIVATE_KEY_FILE,
				PathBuf::from(config_root)
					.join("keys/eth_private_key")
					.to_str()
					.expect("Invalid eth_private_key path"),
			)?
			.set_default(
				ARB_PRIVATE_KEY_FILE,
				PathBuf::from(config_root)
					.join("keys/eth_private_key")
					.to_str()
					.expect("Invalid arb_private_key path"),
			)?
			.set_default(
				SIGNING_DB_FILE,
				PathBuf::from(config_root)
					.join("data.db")
					.to_str()
					.expect("Invalid signing_db_file path"),
			)
	}
}

impl Source for CommandLineOptions {
	fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> std::result::Result<Map<String, Value>, ConfigError> {
		let mut map: HashMap<String, Value> = HashMap::new();

		self.p2p_opts.insert_all(&mut map);

		self.state_chain_opts.insert_all(&mut map);

		self.eth_opts.insert_all(&mut map);

		self.dot_opts.insert_all(&mut map);

		self.btc_opts.insert_all(&mut map);

		self.arb_opts.insert_all(&mut map);

		insert_command_line_option(&mut map, "health_check.hostname", &self.health_check_hostname);
		insert_command_line_option(&mut map, "health_check.port", &self.health_check_port);

		insert_command_line_option(&mut map, "prometheus.hostname", &self.prometheus_hostname);
		insert_command_line_option(&mut map, "prometheus.port", &self.prometheus_port);

		insert_command_line_option_path(&mut map, SIGNING_DB_FILE, &self.signing_db_file);
		insert_command_line_option(
			&mut map,
			LOGGING_SPAN_LIFECYCLE,
			&Some(self.logging_span_lifecycle),
		);
		insert_command_line_option(
			&mut map,
			LOGGING_COMMAND_SERVER_PORT,
			&self.logging_command_server_port,
		);

		Ok(map)
	}
}

/// Inserts the provided option (if Some) as a `config::Value` into the map using the setting_str as
/// the key. Used in the `impl Source for CommandLineOptions` to help build a map of the options.
pub fn insert_command_line_option<T>(
	map: &mut HashMap<String, Value>,
	setting_str: &str,
	option: &Option<T>,
) where
	T: Into<Value> + Clone,
{
	if let Some(value) = option {
		map.insert(setting_str.to_string(), value.clone().into());
	}
}

pub fn insert_command_line_option_path(
	map: &mut HashMap<String, Value>,
	setting_str: &str,
	option: &Option<PathBuf>,
) {
	insert_command_line_option(
		map,
		setting_str,
		&option.as_ref().map(|path| path.to_string_lossy().to_string()),
	);
}

impl StateChainOptions {
	/// Inserts all the State Chain Options into the given map (if Some)
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option(map, STATE_CHAIN_WS_ENDPOINT, &self.state_chain_ws_endpoint);
		insert_command_line_option_path(
			map,
			STATE_CHAIN_SIGNING_KEY_FILE,
			&self.state_chain_signing_key_file,
		);
	}
}

impl EthOptions {
	/// Inserts all the Eth Options into the given map (if Some)
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option(map, "eth.rpc.ws_endpoint", &self.eth_ws_endpoint);
		insert_command_line_option(map, "eth.rpc.http_endpoint", &self.eth_http_endpoint);

		insert_command_line_option(map, "eth.backup_rpc.ws_endpoint", &self.eth_backup_ws_endpoint);
		insert_command_line_option(
			map,
			"eth.backup_rpc.http_endpoint",
			&self.eth_backup_http_endpoint,
		);

		insert_command_line_option_path(map, ETH_PRIVATE_KEY_FILE, &self.eth_private_key_file);
	}
}

impl P2POptions {
	/// Inserts all the P2P Options into the given map (if Some)
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option_path(map, NODE_P2P_KEY_FILE, &self.node_key_file);
		insert_command_line_option(
			map,
			"node_p2p.ip_address",
			&self.ip_address.map(|ip| ip.to_string()),
		);
		insert_command_line_option(map, NODE_P2P_PORT, &self.p2p_port);
		insert_command_line_option(map, NODE_P2P_ALLOW_LOCAL_IP, &self.allow_local_ip);
	}
}

impl BtcOptions {
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option(map, "btc.rpc.http_endpoint", &self.btc_http_endpoint);
		insert_command_line_option(map, "btc.rpc.basic_auth_user", &self.btc_basic_auth_user);
		insert_command_line_option(
			map,
			"btc.rpc.basic_auth_password",
			&self.btc_basic_auth_password,
		);

		insert_command_line_option(
			map,
			"btc.backup_rpc.http_endpoint",
			&self.btc_backup_http_endpoint,
		);
		insert_command_line_option(
			map,
			"btc.backup_rpc.basic_auth_user",
			&self.btc_backup_basic_auth_user,
		);
		insert_command_line_option(
			map,
			"btc.backup_rpc.basic_auth_password",
			&self.btc_backup_basic_auth_password,
		);
	}
}

impl DotOptions {
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option(map, "dot.rpc.ws_endpoint", &self.dot_ws_endpoint);
		insert_command_line_option(map, "dot.rpc.http_endpoint", &self.dot_http_endpoint);

		insert_command_line_option(map, "dot.backup_rpc.ws_endpoint", &self.dot_backup_ws_endpoint);
		insert_command_line_option(
			map,
			"dot.backup_rpc.http_endpoint",
			&self.dot_backup_http_endpoint,
		);
	}
}

impl ArbOptions {
	/// Inserts all the Arb Options into the given map (if Some)
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option(map, "arb.ws_node_endpoint", &self.arb_ws_endpoint);
		insert_command_line_option(map, "arb.http_node_endpoint", &self.arb_http_endpoint);
		insert_command_line_option_path(map, ARB_PRIVATE_KEY_FILE, &self.arb_private_key_file);
	}
}

impl Settings {
	/// New settings loaded from "$base_config_path/config/Settings.toml",
	/// environment and `CommandLineOptions`
	pub fn new(opts: CommandLineOptions) -> Result<Self, ConfigError> {
		Self::new_with_settings_dir(DEFAULT_SETTINGS_DIR, opts)
	}

	pub fn new_with_settings_dir(
		settings_dir: &str,
		opts: CommandLineOptions,
	) -> Result<Self, ConfigError> {
		Self::load_settings_from_all_sources(opts.config_root.clone(), settings_dir, opts)
	}

	#[cfg(test)]
	pub fn new_test() -> Result<Self, ConfigError> {
		Settings::load_settings_from_all_sources(
			env!("CF_TEST_CONFIG_ROOT").to_owned(),
			DEFAULT_SETTINGS_DIR,
			CommandLineOptions::default(),
		)
	}
}

/// Validate a websocket endpoint URL
pub fn validate_websocket_endpoint(url: SecretUrl) -> Result<()> {
	validate_endpoint(vec!["ws", "wss"], url)
}

/// Validate a http endpoint URL
pub fn validate_http_endpoint(url: SecretUrl) -> Result<()> {
	validate_endpoint(vec!["http", "https"], url)
}

/// Parse the URL to check that it is the correct scheme and a valid endpoint URL
fn validate_endpoint(valid_schemes: Vec<&str>, url: SecretUrl) -> Result<()> {
	let parsed_url = Url::parse(url.as_ref()).context(format!("Error parsing url: {url}"))?;
	let scheme = parsed_url.scheme();
	if !valid_schemes.contains(&scheme) {
		bail!("Invalid scheme: `{scheme}` in endpoint: {url}");
	}
	if parsed_url.host().is_none() ||
		parsed_url.fragment().is_some() ||
		parsed_url.cannot_be_a_base()
	{
		bail!("Invalid URL data in endpoint: {url}");
	}

	Ok(())
}

fn is_valid_db_path(db_file: &Path) -> Result<()> {
	if db_file.extension() != Some(OsStr::new("db")) {
		bail!("Db path does not have '.db' extension");
	}
	Ok(())
}

#[cfg(test)]
pub mod tests {
	use utilities::assert_ok;

	use crate::constants::{
		ARB_HTTP_ENDPOINT, ARB_WS_ENDPOINT, BTC_BACKUP_HTTP_ENDPOINT, BTC_BACKUP_RPC_PASSWORD,
		BTC_BACKUP_RPC_USER, BTC_HTTP_ENDPOINT, BTC_RPC_PASSWORD, BTC_RPC_USER,
		DOT_BACKUP_HTTP_ENDPOINT, DOT_BACKUP_WS_ENDPOINT, DOT_HTTP_ENDPOINT, DOT_WS_ENDPOINT,
		ETH_BACKUP_HTTP_ENDPOINT, ETH_BACKUP_WS_ENDPOINT, ETH_HTTP_ENDPOINT, ETH_WS_ENDPOINT,
		NODE_P2P_IP_ADDRESS,
	};

	use super::*;

	macro_rules! implement_test_environment {
		($($const_name:ident => $const_value:expr),*) => {
			pub struct TestEnvironment {}

			impl Default for TestEnvironment {
				fn default() -> TestEnvironment {
					$(
						std::env::set_var($const_name, $const_value);
					)*
					TestEnvironment {}
				}
			}

			impl Drop for TestEnvironment {
				fn drop(&mut self) {
					$(
						std::env::remove_var($const_name);
					)*
				}
			}
		};
	}

	implement_test_environment! {
		ETH_HTTP_ENDPOINT => "http://localhost:8545",
		ETH_WS_ENDPOINT => "ws://localhost:8545",
		ETH_BACKUP_HTTP_ENDPOINT => "http://second.localhost:8545",
		ETH_BACKUP_WS_ENDPOINT => "ws://second.localhost:8545",

		NODE_P2P_IP_ADDRESS => "1.1.1.1",

		BTC_HTTP_ENDPOINT => "http://localhost:18443",
		BTC_RPC_USER => "user",
		BTC_RPC_PASSWORD => "password",

		BTC_BACKUP_HTTP_ENDPOINT => "http://second.localhost:18443",
		BTC_BACKUP_RPC_USER => "second.user",
		BTC_BACKUP_RPC_PASSWORD => "second.password",

		DOT_WS_ENDPOINT => "wss://my_fake_polkadot_rpc:443/<secret_key>",
		DOT_HTTP_ENDPOINT => "https://my_fake_polkadot_rpc:443/<secret_key>",
		DOT_BACKUP_WS_ENDPOINT =>
		"wss://second.my_fake_polkadot_rpc:443/<secret_key>",
		DOT_BACKUP_HTTP_ENDPOINT =>
		"https://second.my_fake_polkadot_rpc:443/<secret_key>",

		ARB_HTTP_ENDPOINT => "http://localhost:8548",
		ARB_WS_ENDPOINT => "ws://localhost:8548"

	}

	// We do them like this so they run sequentially, which is necessary so the environment doesn't
	// interfere with tests running in parallel.
	#[test]
	fn all_settings_tests() {
		settings_valid_if_only_all_the_environment_set();

		test_init_config_with_testing_config();

		test_base_config_path_command_line_option();

		test_all_command_line_options();
	}

	fn settings_valid_if_only_all_the_environment_set() {
		let _guard = TestEnvironment::default();

		let settings = Settings::new(CommandLineOptions::default())
			.expect("Check that the test environment is set correctly");
		assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
		assert_eq!(settings.eth.nodes.primary.http_endpoint.as_ref(), "http://localhost:8545");
		assert_eq!(
			settings.dot.nodes.primary.ws_endpoint.as_ref(),
			"wss://my_fake_polkadot_rpc:443/<secret_key>"
		);
		assert_eq!(
			settings.eth.nodes.backup.unwrap().http_endpoint.as_ref(),
			"http://second.localhost:8545"
		);
		assert_eq!(
			settings.dot.nodes.backup.unwrap().ws_endpoint.as_ref(),
			"wss://second.my_fake_polkadot_rpc:443/<secret_key>"
		);
	}

	fn test_init_config_with_testing_config() {
		let test_settings = Settings::new_test().unwrap();

		assert_eq!(
			test_settings.state_chain.signing_key_file,
			PathBuf::from(env!("CF_TEST_CONFIG_ROOT"))
				.join("keys/alice")
				.canonicalize()
				.unwrap()
		);
	}

	fn test_base_config_path_command_line_option() {
		// Load the settings using a custom base config path.
		let custom_base_path_settings = Settings::new(CommandLineOptions::default()).unwrap();

		// Check that the settings file at "config/testing/config/Settings.toml" was loaded by
		// checking that the `alice` key was loaded rather than the default.
		assert_eq!(
			custom_base_path_settings.state_chain.signing_key_file,
			PathBuf::from(env!("CF_TEST_CONFIG_ROOT"))
				.join("keys/alice")
				.canonicalize()
				.unwrap()
		);

		// Check that a key file is a child of the custom base path.
		// Note: This check will break if the `node_p2p.node_key_file` settings is set in
		// "config/testing/config/Settings.toml".
		assert!(custom_base_path_settings
			.node_p2p
			.node_key_file
			.to_string_lossy()
			.contains(env!("CF_TEST_CONFIG_ROOT")));

		assert_eq!(
			custom_base_path_settings.btc.nodes.primary.http_endpoint,
			"http://localhost:18443".into()
		);
		assert!(custom_base_path_settings.btc.nodes.backup.is_none());
	}

	fn test_all_command_line_options() {
		use std::str::FromStr;
		// Fill the options with test values that will pass the parsing/validation.
		// The test values need to be different from the default values set during `set_defaults()`
		// for the test to work. The `config_root` option is covered in a separate test.
		let opts = CommandLineOptions {
			config_root: CommandLineOptions::default().config_root,
			p2p_opts: P2POptions {
				node_key_file: Some(PathBuf::from_str("keys/node_key_file_2").unwrap()),
				ip_address: Some("1.1.1.1".parse().unwrap()),
				p2p_port: Some(8087),
				allow_local_ip: Some(false),
			},
			state_chain_opts: StateChainOptions {
				state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
				state_chain_signing_key_file: Some(
					PathBuf::from_str("keys/signing_key_file_2").unwrap(),
				),
			},
			eth_opts: EthOptions {
				eth_ws_endpoint: Some("ws://endpoint:4321".to_owned()),
				eth_http_endpoint: Some("http://endpoint:4321".to_owned()),
				eth_backup_ws_endpoint: Some("ws://second_endpoint:4321".to_owned()),
				eth_backup_http_endpoint: Some("http://second_endpoint:4321".to_owned()),
				eth_private_key_file: Some(PathBuf::from_str("keys/eth_private_key_2").unwrap()),
			},
			dot_opts: DotOptions {
				dot_ws_endpoint: Some("ws://endpoint:4321".to_owned()),
				dot_http_endpoint: Some("http://endpoint:4321".to_owned()),

				dot_backup_ws_endpoint: Some("ws://second.endpoint:4321".to_owned()),
				dot_backup_http_endpoint: Some("http://second.endpoint:4321".to_owned()),
			},
			btc_opts: BtcOptions {
				btc_http_endpoint: Some("http://btc-endpoint:4321".to_owned()),
				btc_basic_auth_user: Some("my_username".to_owned()),
				btc_basic_auth_password: Some("my_password".to_owned()),

				btc_backup_http_endpoint: Some("http://second.btc-endpoint:4321".to_owned()),
				btc_backup_basic_auth_user: Some("second.my_username".to_owned()),
				btc_backup_basic_auth_password: Some("second.my_password".to_owned()),
			},
			arb_opts: ArbOptions {
				arb_ws_endpoint: Some("ws://endpoint:4321".to_owned()),
				arb_http_endpoint: Some("http://endpoint:4321".to_owned()),
				arb_private_key_file: Some(PathBuf::from_str("keys/eth_private_key_2").unwrap()),
			},
			health_check_hostname: Some("health_check_hostname".to_owned()),
			health_check_port: Some(1337),
			prometheus_hostname: Some(("prometheus_hostname").to_owned()),
			prometheus_port: Some(9999),
			signing_db_file: Some(PathBuf::from_str("also/not/real.db").unwrap()),
			logging_span_lifecycle: true,
			logging_command_server_port: Some(6969),
		};

		// Load the test opts into the settings
		let settings = Settings::new(opts.clone()).unwrap();

		// Compare the opts and the settings
		assert_eq!(opts.logging_span_lifecycle, settings.logging.span_lifecycle);
		assert_eq!(opts.logging_command_server_port.unwrap(), settings.logging.command_server_port);
		assert!(settings.node_p2p.node_key_file.ends_with("node_key_file_2"));
		assert_eq!(opts.p2p_opts.p2p_port.unwrap(), settings.node_p2p.port);
		assert_eq!(opts.p2p_opts.ip_address.unwrap(), settings.node_p2p.ip_address);
		assert_eq!(opts.p2p_opts.allow_local_ip.unwrap(), settings.node_p2p.allow_local_ip);

		assert_eq!(
			opts.state_chain_opts.state_chain_ws_endpoint.unwrap(),
			settings.state_chain.ws_endpoint
		);
		assert!(settings.state_chain.signing_key_file.ends_with("signing_key_file_2"));

		assert_eq!(
			opts.eth_opts.eth_ws_endpoint.unwrap(),
			settings.eth.nodes.primary.ws_endpoint.as_ref()
		);
		assert_eq!(
			opts.eth_opts.eth_http_endpoint.unwrap(),
			settings.eth.nodes.primary.http_endpoint.as_ref()
		);

		let eth_backup_node = settings.eth.nodes.backup.unwrap();
		assert_eq!(
			opts.eth_opts.eth_backup_ws_endpoint.unwrap(),
			eth_backup_node.ws_endpoint.as_ref()
		);
		assert_eq!(
			opts.eth_opts.eth_backup_http_endpoint.unwrap(),
			eth_backup_node.http_endpoint.as_ref()
		);

		assert!(settings.eth.private_key_file.ends_with("eth_private_key_2"));

		assert_eq!(
			opts.dot_opts.dot_ws_endpoint.unwrap(),
			settings.dot.nodes.primary.ws_endpoint.as_ref()
		);
		assert_eq!(
			opts.dot_opts.dot_http_endpoint.unwrap(),
			settings.dot.nodes.primary.http_endpoint.as_ref()
		);

		let dot_backup_node = settings.dot.nodes.backup.unwrap();
		assert_eq!(
			opts.dot_opts.dot_backup_ws_endpoint.unwrap(),
			dot_backup_node.ws_endpoint.as_ref()
		);
		assert_eq!(
			opts.dot_opts.dot_backup_http_endpoint.unwrap(),
			dot_backup_node.http_endpoint.as_ref()
		);

		assert_eq!(
			opts.btc_opts.btc_http_endpoint.unwrap(),
			settings.btc.nodes.primary.http_endpoint.as_ref()
		);
		assert_eq!(
			opts.btc_opts.btc_basic_auth_user.unwrap(),
			settings.btc.nodes.primary.basic_auth_user
		);
		assert_eq!(
			opts.btc_opts.btc_basic_auth_password.unwrap(),
			settings.btc.nodes.primary.basic_auth_password
		);

		let btc_backup_node = settings.btc.nodes.backup.unwrap();
		assert_eq!(
			opts.btc_opts.btc_backup_basic_auth_user.unwrap(),
			btc_backup_node.basic_auth_user
		);
		assert_eq!(
			opts.btc_opts.btc_backup_basic_auth_password.unwrap(),
			btc_backup_node.basic_auth_password
		);

		assert_eq!(
			opts.health_check_hostname.unwrap(),
			settings.health_check.as_ref().unwrap().hostname
		);
		assert_eq!(opts.health_check_port.unwrap(), settings.health_check.as_ref().unwrap().port);

		assert_eq!(
			opts.prometheus_hostname.unwrap(),
			settings.prometheus.as_ref().unwrap().hostname
		);
		assert_eq!(opts.prometheus_port.unwrap(), settings.prometheus.as_ref().unwrap().port);

		assert!(settings.signing.db_file.ends_with("not/real.db"));
	}

	#[test]
	fn test_websocket_endpoint_url_parsing() {
		assert_ok!(validate_websocket_endpoint(
			"wss://network.my_eth_node:80/d2er2easdfasdfasdf2e".into()
		));
		assert_ok!(validate_websocket_endpoint("wss://network.my_eth_node:80/<secret_key>".into()));
		assert_ok!(validate_websocket_endpoint("wss://network.my_eth_node/<secret_key>".into()));
		assert_ok!(validate_websocket_endpoint("ws://network.my_eth_node/<secret_key>".into()));
		assert_ok!(validate_websocket_endpoint("wss://network.my_eth_node".into()));
		assert_ok!(validate_websocket_endpoint(
			"wss://polkadot.api.onfinality.io:443/ws?apikey=00000000-0000-0000-0000-000000000000"
				.into()
		));
		assert_ok!(validate_websocket_endpoint(
			"wss://username:password@network.my_eth_node:20000".into()
		));
		assert_ok!(validate_websocket_endpoint("ws://username:@END_POINT:20000".into()));
		assert_ok!(validate_websocket_endpoint("wss://:password@network.my_eth_node:20000".into()));
		assert_ok!(validate_websocket_endpoint("ws://@network.my_eth_node:20000".into()));
		assert_ok!(validate_websocket_endpoint("ws://:@network.my_eth_node:20000".into()));
		assert!(validate_websocket_endpoint("https://wrong_scheme.com".into()).is_err());
		assert!(validate_websocket_endpoint("".into()).is_err());
		assert!(validate_websocket_endpoint("wss://username:password@:20000".into()).is_err());
	}

	#[test]
	fn test_http_endpoint_url_parsing() {
		assert_ok!(validate_http_endpoint(
			"http://network.my_eth_node:80/d2er2easdfasdfasdf2e".into()
		));
		assert_ok!(validate_http_endpoint("http://network.my_eth_node:80/<secret_key>".into()));
		assert_ok!(validate_http_endpoint("http://network.my_eth_node/<secret_key>".into()));
		assert_ok!(validate_http_endpoint("https://network.my_eth_node/<secret_key>".into()));
		assert_ok!(validate_http_endpoint("http://network.my_eth_node".into()));
		assert_ok!(validate_http_endpoint(
			"https://username:password@network.my_eth_node:20000".into()
		));
		assert_ok!(validate_http_endpoint("http://username:@END_POINT:20000".into()));
		assert_ok!(validate_http_endpoint("http://:password@network.my_eth_node:20000".into()));
		assert_ok!(validate_http_endpoint("http://@network.my_eth_node:20000".into()));
		assert_ok!(validate_http_endpoint("http://:@network.my_eth_node:20000".into()));
		assert!(validate_http_endpoint("wss://wrong_scheme.com".into()).is_err());
		assert!(validate_http_endpoint("".into()).is_err());
		assert!(validate_http_endpoint("https://username:password@:20000".into()).is_err());
	}

	#[test]
	fn test_db_file_path_parsing() {
		assert_ok!(is_valid_db_path(Path::new("data.db")));
		assert_ok!(is_valid_db_path(Path::new("/my/user/data/data.db")));
		assert!(is_valid_db_path(Path::new("data.errdb")).is_err());
		assert!(is_valid_db_path(Path::new("thishasnoextension")).is_err());
	}

	#[test]
	fn test_dot_port_validation() {
		let valid_settings = Dot {
			nodes: NodeContainer {
				primary: WsHttpEndpoints {
					ws_endpoint: "wss://valid.endpoint_with_port:443/secret_key".into(),
					http_endpoint: "https://valid.endpoint_with_port:443/secret_key".into(),
				},
				backup: Some(WsHttpEndpoints {
					ws_endpoint: "ws://valid.endpoint_with_port:1234".into(),
					http_endpoint: "http://valid.endpoint_with_port:6969".into(),
				}),
			},
		};
		assert_ok!(valid_settings.validate_settings());

		let mut invalid_primary_settings = valid_settings.clone();
		invalid_primary_settings.nodes.primary.ws_endpoint =
			"ws://invalid.no_port_in_url/secret_key".into();
		assert!(invalid_primary_settings.validate_settings().is_err());

		let mut invalid_backup_settings = valid_settings.clone();
		invalid_backup_settings.nodes.backup = Some(WsHttpEndpoints {
			ws_endpoint: "ws://valid.endpoint_with_port:443".into(),
			http_endpoint: "http://invalid.no_port_in_url/secret_key".into(),
		});
		assert!(invalid_backup_settings.validate_settings().is_err());
	}

	#[test]
	fn settings_path_resolution() {
		let config_root = PathBuf::from(env!("CF_TEST_CONFIG_ROOT"));
		let absolute_path_to_settings = config_root.join("config/Settings.toml");

		// Resolving paths.
		assert_eq!(
			resolve_settings_path(
				&config_root,
				&PathBuf::from(""),
				Some(PathResolutionExpectation::ExistingDir)
			)
			.unwrap(),
			PathBuf::from(&config_root),
		);

		// Directory doesn't exist.
		assert!(resolve_settings_path(
			&config_root,
			&PathBuf::from("does/not/exist"),
			Some(PathResolutionExpectation::ExistingDir)
		)
		.expect_err("Expected error when resolving non-existing path")
		.to_string()
		.contains("Path does not exist"));

		// Expect file but existing directory found.
		assert!(resolve_settings_path(
			&config_root,
			&PathBuf::from(""),
			Some(PathResolutionExpectation::ExistingFile)
		)
		.expect_err("Expected error when resolving non-existing file")
		.to_string()
		.contains("is not a file"),);

		// File doesn't exist.
		assert!(resolve_settings_path(
			&config_root,
			&PathBuf::from("config/Setings.toml"),
			Some(PathResolutionExpectation::ExistingFile)
		)
		.expect_err("Expected error when resolving non-existing file")
		.to_string()
		.contains("Path does not exist"),);

		// Resolving files.
		// Relative path:
		assert_eq!(
			resolve_settings_path(
				&config_root,
				&PathBuf::from("config/Settings.toml"),
				Some(PathResolutionExpectation::ExistingFile)
			)
			.unwrap(),
			absolute_path_to_settings,
		);
		// Absolute path:
		assert_eq!(
			resolve_settings_path(
				&config_root,
				&absolute_path_to_settings,
				Some(PathResolutionExpectation::ExistingFile)
			)
			.unwrap(),
			absolute_path_to_settings,
		);

		// Relative path is canonicalized.
		assert_eq!(
			resolve_settings_path(
				&config_root,
				&PathBuf::from("../testing/config/Settings.toml"),
				Some(PathResolutionExpectation::ExistingFile)
			)
			.unwrap(),
			config_root.parent().unwrap().join("testing/config/Settings.toml"),
		);

		// Path not required to exist resolves correctly.
		// Relative:
		assert_eq!(
			resolve_settings_path(&config_root, &PathBuf::from("../path/to/somewhere"), None)
				.unwrap(),
			PathBuf::from(&config_root).join("../path/to/somewhere"),
		);
		// Absolute:
		assert_eq!(
			resolve_settings_path(&config_root, &PathBuf::from("/path/to/somewhere"), None)
				.unwrap(),
			PathBuf::from("/path/to/somewhere"),
		);
	}
}
