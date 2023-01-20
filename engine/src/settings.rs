use std::{
	collections::HashMap,
	ffi::OsStr,
	fmt,
	net::IpAddr,
	path::{Path, PathBuf},
};

use anyhow::bail;
use config::{Config, ConfigBuilder, ConfigError, Environment, File, Map, Source, Value};
use serde::{de, Deserialize, Deserializer};

pub use anyhow::Result;
use sp_runtime::DeserializeOwned;
use url::Url;

use clap::Parser;
use utilities::Port;

use crate::constants::{CONFIG_ROOT, DEFAULT_CONFIG_ROOT};

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
		validate_websocket_endpoint(&self.ws_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Eth {
	pub ws_node_endpoint: String,
	pub http_node_endpoint: String,
	#[serde(deserialize_with = "deser_path")]
	pub private_key_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Dot {
	pub ws_node_endpoint: String,
}

impl Dot {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		validate_websocket_endpoint(&self.ws_node_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

impl Eth {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		validate_websocket_endpoint(&self.ws_node_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		validate_http_endpoint(&self.http_node_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
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

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Log {
	pub whitelist: Vec<String>,
	pub blacklist: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct Settings {
	pub node_p2p: P2P,
	pub state_chain: StateChain,
	pub eth: Eth,

	pub dot: Dot,
	pub health_check: Option<HealthCheck>,
	pub signing: Signing,
	#[serde(default)]
	pub log: Log,
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
	#[clap(long = "eth.ws_node_endpoint")]
	pub eth_ws_node_endpoint: Option<String>,
	#[clap(long = "eth.http_node_endpoint")]
	pub eth_http_node_endpoint: Option<String>,
	#[clap(long = "eth.private_key_file")]
	pub eth_private_key_file: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone, Default)]
pub struct DotOptions {
	pub dot_ws_node_endpoint: Option<String>,
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
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"))]
pub struct CommandLineOptions {
	// Misc Options
	#[clap(short = 'c', long = "config-root", env = CONFIG_ROOT, default_value = DEFAULT_CONFIG_ROOT)]
	config_root: String,
	#[clap(short = 'w', long = "log-whitelist")]
	log_whitelist: Option<Vec<String>>,
	#[clap(short = 'b', long = "log-blacklist")]
	log_blacklist: Option<Vec<String>>,

	#[clap(flatten)]
	p2p_opts: P2POptions,

	#[clap(flatten)]
	state_chain_opts: StateChainOptions,

	#[clap(flatten)]
	eth_opts: EthOptions,

	#[clap(flatten)]
	dot_opts: DotOptions,

	// Health Check Settings
	#[clap(long = "health_check.hostname")]
	health_check_hostname: Option<String>,
	#[clap(long = "health_check.port")]
	health_check_port: Option<Port>,

	// Signing Settings
	#[clap(long = "signing.db_file", parse(from_os_str))]
	signing_db_file: Option<PathBuf>,
}

impl Default for CommandLineOptions {
	fn default() -> Self {
		Self {
			config_root: DEFAULT_CONFIG_ROOT.to_owned(),
			log_whitelist: None,
			log_blacklist: None,
			p2p_opts: P2POptions::default(),
			state_chain_opts: StateChainOptions::default(),
			eth_opts: EthOptions::default(),

			dot_opts: DotOptions::default(),
			health_check_hostname: None,
			health_check_port: None,
			signing_db_file: None,
		}
	}
}

const NODE_P2P_KEY_FILE: &str = "node_p2p.node_key_file";
const NODE_P2P_PORT: &str = "node_p2p.port";
const NODE_P2P_ALLOW_LOCAL_IP: &str = "node_p2p.allow_local_ip";

const STATE_CHAIN_WS_ENDPOINT: &str = "state_chain.ws_endpoint";
const STATE_CHAIN_SIGNING_KEY_FILE: &str = "state_chain.signing_key_file";

const ETH_PRIVATE_KEY_FILE: &str = "eth.private_key_file";

const SIGNING_DB_FILE: &str = "signing.db_file";

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
		opts: Self::CommandLineOptions,
	) -> Result<Self, ConfigError> {
		// Set the default settings
		let mut builder = Self::set_defaults(Config::builder(), &config_root)?;

		// If the file does not exist we will try and continue anyway.
		// Because if all of the settings are covered in the environment, cli options and defaults,
		// then we don't need it.
		let settings_file = PathBuf::from(config_root.clone()).join("config/Settings.toml");
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

		let settings: Self = builder
			.add_source(Environment::default().separator("__"))
			.add_source(opts)
			.build()?
			.try_deserialize()
			.map_err(|e| {
				// Add context to the error message if the settings file was missing.
				ConfigError::Message(if file_present {
					e.to_string()
				} else {
					format!("Config file is missing {}: {}", settings_file.to_string_lossy(), e)
				})
			})?;

		settings.validate_settings()?;

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
	fn validate_settings(&self) -> Result<(), ConfigError>;
}

impl CfSettings for Settings {
	type CommandLineOptions = CommandLineOptions;

	fn validate_settings(&self) -> Result<(), ConfigError> {
		self.eth.validate_settings()?;

		self.dot.validate_settings()?;

		self.state_chain.validate_settings()?;

		is_valid_db_path(self.signing.db_file.as_path())
			.map_err(|e| ConfigError::Message(e.to_string()))
	}

	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
		config_root: &str,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		config_builder
			.set_default(NODE_P2P_ALLOW_LOCAL_IP, false)?
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

		insert_command_line_option(
			&mut map,
			"dot.ws_node_endpoint",
			&self.dot_opts.dot_ws_node_endpoint,
		);

		insert_command_line_option(&mut map, "health_check.hostname", &self.health_check_hostname);
		insert_command_line_option(&mut map, "health_check.port", &self.health_check_port);
		insert_command_line_option_path(&mut map, SIGNING_DB_FILE, &self.signing_db_file);
		insert_command_line_option(&mut map, "log.whitelist", &self.log_whitelist);
		insert_command_line_option(&mut map, "log.blacklist", &self.log_blacklist);

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
		insert_command_line_option(map, "eth.ws_node_endpoint", &self.eth_ws_node_endpoint);
		insert_command_line_option(map, "eth.http_node_endpoint", &self.eth_http_node_endpoint);
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

impl Settings {
	/// New settings loaded from "$base_config_path/config/Settings.toml",
	/// environment and `CommandLineOptions`
	pub fn new(opts: CommandLineOptions) -> Result<Self, ConfigError> {
		Self::load_settings_from_all_sources(opts.config_root.clone(), opts)
	}

	#[cfg(test)]
	pub fn new_test() -> Result<Self, ConfigError> {
		tests::set_test_env();
		Settings::load_settings_from_all_sources(
			"config/testing/".to_owned(),
			CommandLineOptions::default(),
		)
	}
}

/// Validate a websocket endpoint URL
pub fn validate_websocket_endpoint(url: &str) -> Result<()> {
	validate_endpoint(vec!["ws", "wss"], url)
}

/// Validate a http endpoint URL
pub fn validate_http_endpoint(url: &str) -> Result<()> {
	validate_endpoint(vec!["http", "https"], url)
}

/// Parse the URL to check that it is the correct scheme and a valid endpoint URL
fn validate_endpoint(valid_schemes: Vec<&str>, url: &str) -> Result<()> {
	let parsed_url = Url::parse(url)?;
	let scheme = parsed_url.scheme();
	if !valid_schemes.contains(&scheme) {
		bail!("Invalid scheme: `{scheme}`");
	}
	if parsed_url.host().is_none() ||
		parsed_url.username() != "" ||
		parsed_url.password().is_some() ||
		parsed_url.fragment().is_some() ||
		parsed_url.cannot_be_a_base()
	{
		bail!("Invalid URL data.");
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
mod tests {

	use utilities::assert_ok;

	use super::*;
	use std::env;

	pub fn set_test_env() {
		use crate::constants::{ETH_HTTP_NODE_ENDPOINT, ETH_WS_NODE_ENDPOINT, NODE_P2P_IP_ADDRESS};

		env::set_var(ETH_HTTP_NODE_ENDPOINT, "http://localhost:8545");
		env::set_var(ETH_WS_NODE_ENDPOINT, "ws://localhost:8545");
		env::set_var(NODE_P2P_IP_ADDRESS, "1.1.1.1");

		env::set_var("DOT__WS_NODE_ENDPOINT", "wss://my_fake_polkadot_rpc:443/<secret_key>");
	}

	#[test]
	fn init_default_config() {
		set_test_env();

		let settings = Settings::new(CommandLineOptions {
			state_chain_opts: StateChainOptions {
				state_chain_ws_endpoint: None,
				state_chain_signing_key_file: Some(PathBuf::from("")),
			},
			..Default::default()
		})
		.unwrap();
		assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
		assert_eq!(settings.eth.http_node_endpoint, "http://localhost:8545");
	}

	#[test]
	fn test_init_config_with_testing_config() {
		let test_settings = Settings::new_test().unwrap();

		assert_eq!(
			test_settings.state_chain.signing_key_file,
			PathBuf::from("./tests/test_keystore/alice_key")
		);
	}

	#[test]
	fn test_websocket_endpoint_url_parsing() {
		assert_ok!(validate_websocket_endpoint(
			"wss://network.my_eth_node:80/d2er2easdfasdfasdf2e"
		));
		assert_ok!(validate_websocket_endpoint("wss://network.my_eth_node:80/<secret_key>"));
		assert_ok!(validate_websocket_endpoint("wss://network.my_eth_node/<secret_key>"));
		assert_ok!(validate_websocket_endpoint("ws://network.my_eth_node/<secret_key>"));
		assert_ok!(validate_websocket_endpoint("wss://network.my_eth_node"));
		assert_ok!(validate_websocket_endpoint(
			"wss://polkadot.api.onfinality.io:443/ws?apikey=00000000-0000-0000-0000-000000000000"
		));
		assert!(validate_websocket_endpoint("https://wrong_scheme.com").is_err());
		assert!(validate_websocket_endpoint("").is_err());
	}

	#[test]
	fn test_http_endpoint_url_parsing() {
		assert_ok!(validate_http_endpoint("http://network.my_eth_node:80/d2er2easdfasdfasdf2e"));
		assert_ok!(validate_http_endpoint("http://network.my_eth_node:80/<secret_key>"));
		assert_ok!(validate_http_endpoint("http://network.my_eth_node/<secret_key>"));
		assert_ok!(validate_http_endpoint("https://network.my_eth_node/<secret_key>"));
		assert_ok!(validate_http_endpoint("http://network.my_eth_node"));
		assert!(validate_http_endpoint("wss://wrong_scheme.com").is_err());
		assert!(validate_http_endpoint("").is_err());
	}

	#[test]
	fn test_db_file_path_parsing() {
		assert_ok!(is_valid_db_path(Path::new("data.db")));
		assert_ok!(is_valid_db_path(Path::new("/my/user/data/data.db")));
		assert!(is_valid_db_path(Path::new("data.errdb")).is_err());
		assert!(is_valid_db_path(Path::new("thishasnoextension")).is_err());
	}

	#[test]
	fn test_base_config_path_command_line_option() {
		set_test_env();

		// Load the settings using a custom base config path.
		let test_base_config_path = "config/testing/";
		let custom_base_path_settings = Settings::new(CommandLineOptions {
			config_root: test_base_config_path.to_owned(),
			..Default::default()
		})
		.unwrap();

		let default_settings = Settings::new(CommandLineOptions::default()).unwrap();

		// Check that the settings file at "config/testing/config/Settings.toml" was loaded by
		// by comparing it to the default settings. Note: This check will fail if the
		// Settings.toml contains only default or no values.
		assert_ne!(custom_base_path_settings, default_settings);

		// Check that a key file is a child of the custom base path.
		// Note: This check will break if the `node_p2p.node_key_file` settings is set in
		// "config/testing/config/Settings.toml".
		assert!(custom_base_path_settings
			.node_p2p
			.node_key_file
			.to_string_lossy()
			.contains(test_base_config_path));
	}

	#[test]
	fn test_all_command_line_options() {
		use std::str::FromStr;
		// Fill the options with test values that will pass the parsing/validation.
		// The test values need to be different from the default values set during `set_defaults()`
		// for the test to work. The `config_root` option is covered in a separate test.
		let opts = CommandLineOptions {
			config_root: CommandLineOptions::default().config_root,
			log_whitelist: Some(vec!["test1".to_owned()]),
			log_blacklist: Some(vec!["test2".to_owned()]),
			p2p_opts: P2POptions {
				node_key_file: Some(PathBuf::from_str("node_key_file").unwrap()),
				ip_address: Some("1.1.1.1".parse().unwrap()),
				p2p_port: Some(8087),
				allow_local_ip: Some(false),
			},
			state_chain_opts: StateChainOptions {
				state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
				state_chain_signing_key_file: Some(PathBuf::from_str("signing_key_file").unwrap()),
			},
			eth_opts: EthOptions {
				eth_ws_node_endpoint: Some("ws://endpoint:4321".to_owned()),
				eth_http_node_endpoint: Some("http://endpoint:4321".to_owned()),
				eth_private_key_file: Some(PathBuf::from_str("eth_key_file").unwrap()),
			},

			dot_opts: DotOptions { dot_ws_node_endpoint: Some("ws://endpoint:4321".to_owned()) },
			health_check_hostname: Some("health_check_hostname".to_owned()),
			health_check_port: Some(1337),
			signing_db_file: Some(PathBuf::from_str("also/not/real.db").unwrap()),
		};

		// Load the test opts into the settings
		let settings = Settings::new(opts.clone()).unwrap();

		// Compare the opts and the settings
		assert_eq!(opts.p2p_opts.node_key_file.unwrap(), settings.node_p2p.node_key_file);
		assert_eq!(opts.p2p_opts.p2p_port.unwrap(), settings.node_p2p.port);
		assert_eq!(opts.p2p_opts.ip_address.unwrap(), settings.node_p2p.ip_address);
		assert_eq!(opts.p2p_opts.allow_local_ip.unwrap(), settings.node_p2p.allow_local_ip);

		assert_eq!(
			opts.state_chain_opts.state_chain_ws_endpoint.unwrap(),
			settings.state_chain.ws_endpoint
		);
		assert_eq!(
			opts.state_chain_opts.state_chain_signing_key_file.unwrap(),
			settings.state_chain.signing_key_file
		);

		assert_eq!(opts.eth_opts.eth_ws_node_endpoint.unwrap(), settings.eth.ws_node_endpoint);
		assert_eq!(opts.eth_opts.eth_http_node_endpoint.unwrap(), settings.eth.http_node_endpoint);
		assert_eq!(opts.eth_opts.eth_private_key_file.unwrap(), settings.eth.private_key_file);

		assert_eq!(opts.dot_opts.dot_ws_node_endpoint.unwrap(), settings.dot.ws_node_endpoint);

		assert_eq!(
			opts.health_check_hostname.unwrap(),
			settings.health_check.as_ref().unwrap().hostname
		);
		assert_eq!(opts.health_check_port.unwrap(), settings.health_check.as_ref().unwrap().port);

		assert_eq!(opts.signing_db_file.unwrap(), settings.signing.db_file);

		assert_eq!(opts.log_whitelist.unwrap(), settings.log.whitelist);
		assert_eq!(opts.log_blacklist.unwrap(), settings.log.blacklist);
	}
}
