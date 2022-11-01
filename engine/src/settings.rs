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

#[derive(Debug, Deserialize, Clone)]
pub struct P2P {
	#[serde(deserialize_with = "deser_path")]
	pub node_key_file: PathBuf,
	pub ip_address: IpAddr,
	pub port: Port,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct StateChain {
	pub ws_endpoint: String,
	#[serde(deserialize_with = "deser_path")]
	pub signing_key_file: PathBuf,
}

impl StateChain {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		parse_websocket_endpoint(&self.ws_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Eth {
	pub ws_node_endpoint: String,
	pub http_node_endpoint: String,
	#[serde(deserialize_with = "deser_path")]
	pub private_key_file: PathBuf,
}

#[cfg(feature = "ibiza")]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct Dot {
	pub ws_node_endpoint: String,
}

#[cfg(feature = "ibiza")]
impl Dot {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		parse_websocket_endpoint(&self.ws_node_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

impl Eth {
	pub fn validate_settings(&self) -> Result<(), ConfigError> {
		parse_websocket_endpoint(&self.ws_node_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		parse_http_endpoint(&self.http_node_endpoint)
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct HealthCheck {
	pub hostname: String,
	pub port: Port,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Signing {
	#[serde(deserialize_with = "deser_path")]
	pub db_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Log {
	pub whitelist: Vec<String>,
	pub blacklist: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
	pub node_p2p: P2P,
	pub state_chain: StateChain,
	pub eth: Eth,
	#[cfg(feature = "ibiza")]
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

#[cfg(feature = "ibiza")]
#[derive(Parser, Debug, Clone, Default)]
pub struct DotOptions {
	pub dot_ws_node_endpoint: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct CommandLineOptions {
	// Misc Options
	#[clap(short = 'c', long = "config-path")]
	config_path: Option<String>,
	#[clap(short = 'w', long = "log-whitelist")]
	log_whitelist: Option<Vec<String>>,
	#[clap(short = 'b', long = "log-blacklist")]
	log_blacklist: Option<Vec<String>>,

	// P2P Settings
	#[clap(long = "p2p.node_key_file", parse(from_os_str))]
	node_key_file: Option<PathBuf>,
	#[clap(long = "p2p.ip_address")]
	ip_address: Option<IpAddr>,
	#[clap(long = "p2p.port")]
	p2p_port: Option<Port>,

	#[clap(flatten)]
	state_chain_opts: StateChainOptions,

	#[clap(flatten)]
	eth_opts: EthOptions,

	#[cfg(feature = "ibiza")]
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

const HEALTH_CHECK_HOSTNAME: &str = "health_check.hostname";
const HEALTH_CHECK_PORT: &str = "health_check.port";

impl CommandLineOptions {
	/// Creates an empty CommandLineOptions with `None` for all fields
	pub fn new() -> CommandLineOptions {
		CommandLineOptions {
			config_path: None,
			log_whitelist: None,
			log_blacklist: None,
			node_key_file: None,
			ip_address: None,
			p2p_port: None,
			state_chain_opts: StateChainOptions::default(),
			eth_opts: EthOptions::default(),
			#[cfg(feature = "ibiza")]
			dot_opts: DotOptions::default(),
			health_check_hostname: None,
			health_check_port: None,
			signing_db_file: None,
		}
	}
}

impl Default for CommandLineOptions {
	fn default() -> Self {
		Self::new()
	}
}

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

	/// Uses the `default_file` unless the `optional_file` is Some.
	/// Merges settings from a TOML file, environment and provided command line options.
	/// Merge priority is:
	/// 1 - Command line options
	/// 2 - Environment
	/// 3 - TOML file
	fn load_settings_from_all_sources(
		default_file: &str,
		optional_file: Option<String>,
		opts: Self::CommandLineOptions,
	) -> Result<Self, ConfigError> {
		// Set the custom default settings
		let mut builder = Self::set_defaults(Config::builder())?;

		// Choose what file to use
		let file = match &optional_file {
			Some(path) => {
				if Path::new(path).is_file() {
					path
				} else {
					// If the user has set the config file path, then error if its missing.
					return Err(ConfigError::Message(format!("File not found: {}", path)))
				}
			},
			None => default_file,
		};

		// If the file does not exist we will try and continue anyway.
		// Because if all of the settings are covered in the environment and cli options, then we
		// don't need it.
		let file_present = Path::new(file).is_file();
		if file_present {
			builder = builder.add_source(File::with_name(file));
		}

		let settings: Self = builder
			.add_source(Environment::default().separator("__"))
			.add_source(opts)
			.build()?
			.try_deserialize()
			.map_err(|e| {
				// Add context to the error message if the file was missing.
				ConfigError::Message(if file_present {
					e.to_string()
				} else {
					format!("Default config file is missing {}: {}", file, e)
				})
			})?;

		settings.validate_settings()?;

		Ok(settings)
	}

	/// Set the default values of any settings. These values will be overridden by all other
	/// sources. Any set this way will become optional (If no other source contains the settings, it
	/// will NOT panic).
	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
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

		#[cfg(feature = "ibiza")]
		self.dot.validate_settings()?;

		self.state_chain.validate_settings()?;

		is_valid_db_path(self.signing.db_file.as_path())
			.map_err(|e| ConfigError::Message(e.to_string()))
	}

	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		config_builder
			.set_default(HEALTH_CHECK_HOSTNAME, HealthCheck::default().hostname)?
			.set_default(HEALTH_CHECK_PORT, HealthCheck::default().port)
	}
}

impl Source for CommandLineOptions {
	fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> std::result::Result<Map<String, Value>, ConfigError> {
		let mut map: HashMap<String, Value> = HashMap::new();

		insert_command_line_option_path(&mut map, "node_p2p.node_key_file", &self.node_key_file);

		insert_command_line_option(
			&mut map,
			"node_p2p.ip_address",
			&self.ip_address.map(|ip| ip.to_string()),
		);

		insert_command_line_option(&mut map, "node_p2p.port", &self.p2p_port);

		self.state_chain_opts.insert_all(&mut map);

		self.eth_opts.insert_all(&mut map);

		#[cfg(feature = "ibiza")]
		insert_command_line_option(
			&mut map,
			"dot.ws_node_endpoint",
			&self.dot_opts.dot_ws_node_endpoint,
		);

		insert_command_line_option(&mut map, HEALTH_CHECK_HOSTNAME, &self.health_check_hostname);
		insert_command_line_option(&mut map, HEALTH_CHECK_PORT, &self.health_check_port);
		insert_command_line_option_path(&mut map, "signing.db_file", &self.signing_db_file);
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
		insert_command_line_option(map, "state_chain.ws_endpoint", &self.state_chain_ws_endpoint);
		insert_command_line_option_path(
			map,
			"state_chain.signing_key_file",
			&self.state_chain_signing_key_file,
		);
	}
}

impl EthOptions {
	/// Inserts all the Eth Shared Options into the given map (if Some)
	pub fn insert_all(&self, map: &mut HashMap<String, Value>) {
		insert_command_line_option(map, "eth.ws_node_endpoint", &self.eth_ws_node_endpoint);
		insert_command_line_option(map, "eth.http_node_endpoint", &self.eth_http_node_endpoint);
		insert_command_line_option_path(map, "eth.private_key_file", &self.eth_private_key_file);
	}
}

impl Settings {
	/// New settings loaded from the `config_path` in the `CommandLineOptions` or
	/// "config/Default.toml" if none, with overridden values from the environment and
	/// `CommandLineOptions`
	pub fn new(opts: CommandLineOptions) -> Result<Self, ConfigError> {
		#[cfg(not(feature = "ibiza"))]
		let default_settings = "config/Default.toml";
		#[cfg(feature = "ibiza")]
		let default_settings = "config/IbizaDefault.toml";
		Self::load_settings_from_all_sources(default_settings, opts.config_path.clone(), opts)
	}

	#[cfg(test)]
	pub fn new_test() -> Result<Self, ConfigError> {
		tests::set_test_env();
		Settings::load_settings_from_all_sources(
			"config/Testing.toml",
			None,
			CommandLineOptions::default(),
		)
	}
}

/// Validate a websocket endpoint URL
pub fn parse_websocket_endpoint(url: &str) -> Result<Url> {
	parse_endpoint(vec!["ws", "wss"], url)
}

/// Validate a http endpoint URL
pub fn parse_http_endpoint(url: &str) -> Result<Url> {
	parse_endpoint(vec!["http", "https"], url)
}

/// Parse the URL to check that it is the correct scheme and a valid endpoint URL
fn parse_endpoint(valid_schemes: Vec<&str>, url: &str) -> Result<Url> {
	let parsed_url = Url::parse(url)?;
	let scheme = parsed_url.scheme();
	if !valid_schemes.contains(&scheme) {
		bail!("Invalid scheme: `{}`", scheme);
	}
	if parsed_url.host() == None ||
		parsed_url.username() != "" ||
		parsed_url.password() != None ||
		parsed_url.query() != None ||
		parsed_url.fragment() != None ||
		parsed_url.cannot_be_a_base()
	{
		bail!("Invalid URL data.");
	}

	Ok(parsed_url)
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

	pub fn set_test_env() {
		use std::env;

		use crate::constants::{
			ETH_HTTP_NODE_ENDPOINT, ETH_WS_NODE_ENDPOINT, NODE_P2P_IP_ADDRESS, NODE_P2P_PORT,
		};

		env::set_var(ETH_HTTP_NODE_ENDPOINT, "http://localhost:8545");
		env::set_var(ETH_WS_NODE_ENDPOINT, "ws://localhost:8545");
		env::set_var(NODE_P2P_IP_ADDRESS, "1.1.1.1");
		env::set_var(NODE_P2P_PORT, "8087");
	}

	#[test]
	fn init_default_config() {
		set_test_env();
		let settings = Settings::new(CommandLineOptions::default()).unwrap();
		assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
		assert_eq!(settings.eth.http_node_endpoint, "http://localhost:8545");
	}

	#[test]
	fn test_init_config_with_testing_config() {
		let test_settings = Settings::new_test().unwrap();

		assert_eq!(test_settings.state_chain.ws_endpoint, "ws://localhost:9944");
	}

	#[test]
	fn test_websocket_endpoint_url_parsing() {
		assert_ok!(parse_websocket_endpoint("wss://network.my_eth_node:80/d2er2easdfasdfasdf2e"));
		assert_ok!(parse_websocket_endpoint("wss://network.my_eth_node:80/<secret_key>"));
		assert_ok!(parse_websocket_endpoint("wss://network.my_eth_node/<secret_key>"));
		assert_ok!(parse_websocket_endpoint("ws://network.my_eth_node/<secret_key>"));
		assert_ok!(parse_websocket_endpoint("wss://network.my_eth_node"));
		assert!(parse_websocket_endpoint("https://wrong_scheme.com").is_err());
		assert!(parse_websocket_endpoint("").is_err());
	}

	#[test]
	fn test_http_endpoint_url_parsing() {
		assert_ok!(parse_http_endpoint("http://network.my_eth_node:80/d2er2easdfasdfasdf2e"));
		assert_ok!(parse_http_endpoint("http://network.my_eth_node:80/<secret_key>"));
		assert_ok!(parse_http_endpoint("http://network.my_eth_node/<secret_key>"));
		assert_ok!(parse_http_endpoint("https://network.my_eth_node/<secret_key>"));
		assert_ok!(parse_http_endpoint("http://network.my_eth_node"));
		assert!(parse_http_endpoint("wss://wrong_scheme.com").is_err());
		assert!(parse_http_endpoint("").is_err());
	}

	#[test]
	fn test_db_file_path_parsing() {
		assert_ok!(is_valid_db_path(Path::new("data.db")));
		assert_ok!(is_valid_db_path(Path::new("/my/user/data/data.db")));
		assert!(is_valid_db_path(Path::new("data.errdb")).is_err());
		assert!(is_valid_db_path(Path::new("thishasnoextension")).is_err());
	}

	#[test]
	fn test_config_command_line_option() {
		// Load both the settings files using the --config command line option
		let mut opts = CommandLineOptions::new();
		opts.config_path = Some("config/Testing.toml".to_owned());

		let settings1 = Settings::new(opts).unwrap();

		let mut opts = CommandLineOptions::new();

		#[cfg(not(feature = "ibiza"))]
		let default_file = "config/Default.toml";
		#[cfg(feature = "ibiza")]
		let default_file = "config/IbizaDefault.toml";

		opts.config_path = Some(default_file.to_owned());

		let settings2 = Settings::new(opts).unwrap();

		// Now compare a value that should be different to confirm that both files loaded.
		// Note: This test will break/fail if the Testing.toml and Default.toml have the same
		// `signing_key_file` value
		assert_ne!(settings1.state_chain.signing_key_file, settings2.state_chain.signing_key_file);
	}

	#[test]
	fn test_all_command_line_options() {
		use std::str::FromStr;

		// Fill the options with test values that will pass the parsing/validation.
		// The test values need to be different from the values in `Default.toml` for the test to
		// work. Leave the `config_path` option out, it is covered in a separate test.
		let opts = CommandLineOptions {
			config_path: None,
			log_whitelist: Some(vec!["test1".to_owned()]),
			log_blacklist: Some(vec!["test2".to_owned()]),
			node_key_file: Some(PathBuf::from_str("node_key_file").unwrap()),
			ip_address: Some("1.1.1.1".parse().unwrap()),
			p2p_port: Some(8087),
			state_chain_opts: StateChainOptions {
				state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
				state_chain_signing_key_file: Some(PathBuf::from_str("signing_key_file").unwrap()),
			},
			eth_opts: EthOptions {
				eth_ws_node_endpoint: Some("ws://endpoint:4321".to_owned()),
				eth_http_node_endpoint: Some("http://endpoint:4321".to_owned()),
				eth_private_key_file: Some(PathBuf::from_str("eth_key_file").unwrap()),
			},
			#[cfg(feature = "ibiza")]
			dot_opts: DotOptions { dot_ws_node_endpoint: Some("ws://endpoint:4321".to_owned()) },
			health_check_hostname: Some("health_check_hostname".to_owned()),
			health_check_port: Some(1337),
			signing_db_file: Some(PathBuf::from_str("also/not/real.db").unwrap()),
		};

		// Load the test opts into the settings
		let settings = Settings::new(opts.clone()).unwrap();

		// Compare the opts and the settings
		assert_eq!(opts.node_key_file.unwrap(), settings.node_p2p.node_key_file);

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
