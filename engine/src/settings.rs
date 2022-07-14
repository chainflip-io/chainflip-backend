use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};

use config::{Config, ConfigError, Environment, File};
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

impl Eth {
    pub fn validate_settings(&self) -> Result<(), ConfigError> {
        parse_websocket_endpoint(&self.ws_node_endpoint)
            .map_err(|e| ConfigError::Message(e.to_string()))?;
        parse_http_endpoint(&self.http_node_endpoint)
            .map_err(|e| ConfigError::Message(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
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
pub struct EthSharedOptions {
    #[clap(long = "eth.ws_node_endpoint")]
    pub eth_ws_node_endpoint: Option<String>,
    #[clap(long = "eth.http_node_endpoint")]
    pub eth_http_node_endpoint: Option<String>,
    #[clap(long = "eth.private_key_file")]
    pub eth_private_key_file: Option<PathBuf>,
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

    #[clap(flatten)]
    state_chain_opts: StateChainOptions,

    #[clap(flatten)]
    eth_opts: EthSharedOptions,

    // Health Check Settings
    #[clap(long = "health_check.hostname")]
    health_check_hostname: Option<String>,
    #[clap(long = "health_check.port")]
    health_check_port: Option<Port>,

    // Signing Settings
    #[clap(long = "signing.db_file", parse(from_os_str))]
    signing_db_file: Option<PathBuf>,
}

impl CommandLineOptions {
    /// Creates an empty CommandLineOptions with `None` for all fields
    pub fn new() -> CommandLineOptions {
        CommandLineOptions {
            config_path: None,
            log_whitelist: None,
            log_blacklist: None,
            node_key_file: None,
            state_chain_opts: StateChainOptions::default(),
            eth_opts: EthSharedOptions::default(),
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
pub trait CfSettings {
    type Settings: DeserializeOwned;

    /// Deserialize the TOML file pointed to by `path` into a `Settings` struct
    /// If an item exists in the file *and* the environment, the environment overrides the file.
    fn settings_from_file_and_env(file: &str) -> Result<Self::Settings, ConfigError> {
        Config::builder()
            .add_source(File::with_name(file))
            .add_source(Environment::default().separator("__"))
            .build()?
            .try_deserialize()
    }

    /// Validate the formatting of some settings
    fn validate_settings(&self) -> Result<(), ConfigError>;
}

impl CfSettings for Settings {
    type Settings = Self;

    fn validate_settings(&self) -> Result<(), ConfigError> {
        self.eth.validate_settings()?;

        self.state_chain.validate_settings()?;

        is_valid_db_path(self.signing.db_file.as_path())
            .map_err(|e| ConfigError::Message(e.to_string()))
    }
}

impl Settings {
    /// New settings loaded from the provided path or "config/Default.toml" with overridden values from the `CommandLineOptions`
    pub fn new(opts: CommandLineOptions) -> Result<Self, ConfigError> {
        let settings = Self::from_file_and_env(
            match &opts.config_path.clone() {
                Some(path) => path,
                None => "config/Default.toml",
            },
            opts,
        )?;

        Ok(settings)
    }

    #[cfg(test)]
    pub fn new_test() -> Result<Self, ConfigError> {
        tests::set_test_env();
        Settings::from_file_and_env("config/Testing.toml", CommandLineOptions::default())
    }

    /// Load settings from a TOML file
    /// If opts contains another file name, it'll use that as the default
    pub fn from_file_and_env(file: &str, opts: CommandLineOptions) -> Result<Self, ConfigError> {
        let mut settings = Self::settings_from_file_and_env(file)?;

        if let Some(opt) = opts.node_key_file {
            settings.node_p2p.node_key_file = opt
        };

        // State Chain
        if let Some(opt) = opts.state_chain_opts.state_chain_ws_endpoint {
            settings.state_chain.ws_endpoint = opt
        };
        if let Some(opt) = opts.state_chain_opts.state_chain_signing_key_file {
            settings.state_chain.signing_key_file = opt
        };

        // Eth
        if let Some(opt) = opts.eth_opts.eth_ws_node_endpoint {
            settings.eth.ws_node_endpoint = opt
        };

        if let Some(opt) = opts.eth_opts.eth_http_node_endpoint {
            settings.eth.http_node_endpoint = opt
        };

        if let Some(opt) = opts.eth_opts.eth_private_key_file {
            settings.eth.private_key_file = opt
        };

        // Health Check - this is optional
        let mut health_check = HealthCheck::default();
        if let Some(opt) = opts.health_check_hostname {
            health_check.hostname = opt;
        };
        if let Some(opt) = opts.health_check_port {
            health_check.port = opt;
        };
        // Don't override the healthcheck settings unless something has changed
        if health_check != HealthCheck::default() {
            settings.health_check = Some(health_check);
        }

        // Signing
        if let Some(opt) = opts.signing_db_file {
            settings.signing.db_file = opt
        };

        // log
        if let Some(opt) = opts.log_whitelist {
            settings.log.whitelist = opt;
        };
        if let Some(opt) = opts.log_blacklist {
            settings.log.blacklist = opt;
        };

        settings.validate_settings()?;

        Ok(settings)
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
        return Err(anyhow::Error::msg(format!("Invalid scheme: `{}`", scheme)));
    }
    if parsed_url.host() == None
        || parsed_url.username() != ""
        || parsed_url.password() != None
        || parsed_url.query() != None
        || parsed_url.fragment() != None
        || parsed_url.cannot_be_a_base()
    {
        return Err(anyhow::Error::msg("Invalid URL data."));
    }

    Ok(parsed_url)
}

fn is_valid_db_path(db_file: &Path) -> Result<()> {
    if db_file.extension() != Some(OsStr::new("db")) {
        return Err(anyhow::Error::msg("Db path does not have '.db' extension"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use utilities::assert_ok;

    use super::*;

    pub fn set_test_env() {
        use std::env;

        use crate::constants::{ETH_HTTP_NODE_ENDPOINT, ETH_WS_NODE_ENDPOINT};

        env::set_var(ETH_HTTP_NODE_ENDPOINT, "http://localhost:8545");
        env::set_var(ETH_WS_NODE_ENDPOINT, "ws://localhost:8545");
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
        assert_ok!(parse_websocket_endpoint(
            "wss://network.my_eth_node:80/d2er2easdfasdfasdf2e"
        ));
        assert_ok!(parse_websocket_endpoint(
            "wss://network.my_eth_node:80/<secret_key>"
        ));
        assert_ok!(parse_websocket_endpoint(
            "wss://network.my_eth_node/<secret_key>"
        ));
        assert_ok!(parse_websocket_endpoint(
            "ws://network.my_eth_node/<secret_key>"
        ));
        assert_ok!(parse_websocket_endpoint("wss://network.my_eth_node"));
        assert!(parse_websocket_endpoint("https://wrong_scheme.com").is_err());
        assert!(parse_websocket_endpoint("").is_err());
    }

    #[test]
    fn test_http_endpoint_url_parsing() {
        assert_ok!(parse_http_endpoint(
            "http://network.my_eth_node:80/d2er2easdfasdfasdf2e"
        ));
        assert_ok!(parse_http_endpoint(
            "http://network.my_eth_node:80/<secret_key>"
        ));
        assert_ok!(parse_http_endpoint(
            "http://network.my_eth_node/<secret_key>"
        ));
        assert_ok!(parse_http_endpoint(
            "https://network.my_eth_node/<secret_key>"
        ));
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
        opts.config_path = Some("config/Default.toml".to_owned());

        let settings2 = Settings::new(opts).unwrap();

        // Now compare a value that should be different to confirm that both files loaded.
        // Note: This test will break/fail if the Testing.toml and Default.toml have the same `signing_key_file` value
        assert_ne!(
            settings1.state_chain.signing_key_file,
            settings2.state_chain.signing_key_file
        );
    }

    #[test]
    fn test_all_command_line_options() {
        use std::str::FromStr;

        // Fill the options with junk values that will pass the parsing/validation.
        // The junk values need to be different from the values in `Default.toml` for the test to work.
        // Leave the `config_path` option out, it is covered in a separate test.
        let opts = CommandLineOptions {
            config_path: None,
            log_whitelist: Some(vec!["test1".to_owned()]),
            log_blacklist: Some(vec!["test2".to_owned()]),
            node_key_file: Some(PathBuf::from_str("node_key_file").unwrap()),
            state_chain_opts: StateChainOptions {
                state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
                state_chain_signing_key_file: Some(PathBuf::from_str("signing_key_file").unwrap()),
            },
            eth_opts: EthSharedOptions {
                eth_ws_node_endpoint: Some("ws://endpoint:4321".to_owned()),
                eth_http_node_endpoint: Some("http://endpoint:4321".to_owned()),
                eth_private_key_file: Some(PathBuf::from_str("not/a/real/path.toml").unwrap()),
            },
            health_check_hostname: Some("health_check_hostname".to_owned()),
            health_check_port: Some(1337),
            signing_db_file: Some(PathBuf::from_str("also/not/real.db").unwrap()),
        };

        // Load the junk opts into the settings
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

        assert_eq!(
            opts.eth_opts.eth_ws_node_endpoint.unwrap(),
            settings.eth.ws_node_endpoint
        );
        assert_eq!(
            opts.eth_opts.eth_http_node_endpoint.unwrap(),
            settings.eth.http_node_endpoint
        );
        assert_eq!(
            opts.eth_opts.eth_private_key_file.unwrap(),
            settings.eth.private_key_file
        );

        assert_eq!(
            opts.health_check_hostname.unwrap(),
            settings.health_check.as_ref().unwrap().hostname
        );
        assert_eq!(
            opts.health_check_port.unwrap(),
            settings.health_check.as_ref().unwrap().port
        );

        assert_eq!(opts.signing_db_file.unwrap(), settings.signing.db_file);

        assert_eq!(opts.log_whitelist.unwrap(), settings.log.whitelist);
        assert_eq!(opts.log_blacklist.unwrap(), settings.log.blacklist);
    }
}
