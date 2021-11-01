use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};

use config::{Config, ConfigError, File};
use serde::{de, Deserialize, Deserializer};
use web3::types::H160;

pub use anyhow::Result;
use url::Url;

use structopt::StructOpt;

#[derive(Debug, Deserialize, Clone)]
pub struct StateChain {
    pub ws_endpoint: String,
    pub signing_key_file: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
    pub from_block: u64,
    pub node_endpoint: String,
    pub stake_manager_eth_address: H160,
    pub key_manager_eth_address: H160,
    #[serde(deserialize_with = "deser_path")]
    pub private_key_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    pub hostname: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Signing {
    #[serde(deserialize_with = "deser_path")]
    pub db_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub state_chain: StateChain,
    pub eth: Eth,
    pub health_check: HealthCheck,
    pub signing: Signing,
}

#[derive(StructOpt, Debug, Clone)]
pub struct CommandLineOptions {
    // Misc Options
    #[structopt(short = "c", long = "config-path")]
    config_path: Option<String>,

    // State Chain Settings
    #[structopt(long = "state_chain.ws_endpoint")]
    state_chain_ws_endpoint: Option<String>,
    #[structopt(long = "state_chain.signing_key_file")]
    state_chain_signing_key_file: Option<String>,

    // Eth Settings
    #[structopt(long = "eth.from_block")]
    eth_from_block: Option<u64>,
    #[structopt(long = "eth.node_endpoint")]
    eth_node_endpoint: Option<String>,
    #[structopt(long = "eth.stake_manager_eth_address")]
    eth_stake_manager_eth_address: Option<H160>,
    #[structopt(long = "eth.key_manager_eth_address")]
    eth_key_manager_eth_address: Option<H160>,
    #[structopt(long = "eth.private_key_file", parse(from_os_str))]
    eth_private_key_file: Option<PathBuf>,

    // Health Check Settings
    #[structopt(long = "health_check.hostname")]
    health_check_hostname: Option<String>,
    #[structopt(long = "health_check.port")]
    health_check_port: Option<u16>,

    // Singing Settings
    #[structopt(long = "signing.db_file", parse(from_os_str))]
    signing_db_file: Option<PathBuf>,
}

impl CommandLineOptions {
    /// Creates an empty CommandLineOptions with `None` for all fields
    pub fn new() -> CommandLineOptions {
        CommandLineOptions {
            config_path: None,
            state_chain_ws_endpoint: None,
            state_chain_signing_key_file: None,
            eth_from_block: None,
            eth_node_endpoint: None,
            eth_stake_manager_eth_address: None,
            eth_key_manager_eth_address: None,
            eth_private_key_file: None,
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

impl Settings {
    /// New settings loaded from "config/Default.toml" with overridden values from the `CommandLineOptions`
    pub fn new(opts: CommandLineOptions) -> Result<Self, ConfigError> {
        // Load settings from the default file or from the path specified from cmd line options
        let mut settings = match opts.config_path {
            Some(path) => Self::from_file(&path)?,
            None => Self::from_file("config/Default.toml")?,
        };

        // Override the settings with the cmd line options
        // State Chain
        if let Some(opt) = opts.state_chain_ws_endpoint {
            settings.state_chain.ws_endpoint = opt
        };
        if let Some(opt) = opts.state_chain_signing_key_file {
            settings.state_chain.signing_key_file = opt
        };

        // Eth
        if let Some(opt) = opts.eth_from_block {
            settings.eth.from_block = opt
        };
        if let Some(opt) = opts.eth_node_endpoint {
            settings.eth.node_endpoint = opt
        };
        if let Some(opt) = opts.eth_stake_manager_eth_address {
            settings.eth.stake_manager_eth_address = opt
        };
        if let Some(opt) = opts.eth_key_manager_eth_address {
            settings.eth.key_manager_eth_address = opt
        };
        if let Some(opt) = opts.eth_private_key_file {
            settings.eth.private_key_file = opt
        };

        // Health Check
        if let Some(opt) = opts.health_check_hostname {
            settings.health_check.hostname = opt
        };
        if let Some(opt) = opts.health_check_port {
            settings.health_check.port = opt
        };

        // Signing
        if let Some(opt) = opts.signing_db_file {
            settings.signing.db_file = opt
        };

        // Run the validation again
        settings.validate_settings()?;

        Ok(settings)
    }

    /// Validates the formatting of some settings
    pub fn validate_settings(&self) -> Result<(), ConfigError> {
        parse_websocket_url(&self.eth.node_endpoint)
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        parse_websocket_url(&self.state_chain.ws_endpoint)
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        is_valid_db_path(self.signing.db_file.as_path())
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        Ok(())
    }

    /// Load settings from a TOML file
    pub fn from_file(file: &str) -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // merging in the configuration file
        s.merge(File::with_name(file))?;

        // You can deserialize (and thus freeze) the entire configuration as
        let s: Settings = s.try_into()?;

        // make sure the settings are clean
        s.validate_settings()?;

        Ok(s)
    }
}

/// Parse the URL and check that it is a valid websocket url
pub fn parse_websocket_url(url: &str) -> Result<Url> {
    let issue_list_url = Url::parse(&url)?;
    if issue_list_url.scheme() != "ws" && issue_list_url.scheme() != "wss" {
        return Err(anyhow::Error::msg("Wrong scheme"));
    }
    if issue_list_url.host() == None
        || issue_list_url.username() != ""
        || issue_list_url.password() != None
        || issue_list_url.query() != None
        || issue_list_url.fragment() != None
        || issue_list_url.cannot_be_a_base()
    {
        return Err(anyhow::Error::msg("Invalid URL data"));
    }

    Ok(issue_list_url)
}

fn is_valid_db_path(db_file: &Path) -> Result<()> {
    if db_file.extension() != Some(OsStr::new("db")) {
        return Err(anyhow::Error::msg("Db path does not have '.db' extension"));
    }
    Ok(())
}

#[cfg(test)]
pub mod test_utils {
    use super::*;

    /// Loads the settings from the "config/Testing.toml" file
    pub fn new_test_settings() -> Result<Settings, ConfigError> {
        Settings::from_file("config/Testing.toml")
    }
}

#[cfg(test)]
mod tests {

    use crate::testing::assert_ok;

    use super::*;

    #[test]
    fn init_default_config() {
        let settings = Settings::new(CommandLineOptions::new()).unwrap();

        assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
    }

    #[test]
    fn test_init_config_with_testing_config() {
        let test_settings = test_utils::new_test_settings().unwrap();

        assert_eq!(test_settings.state_chain.ws_endpoint, "ws://localhost:9944");
    }

    #[test]
    fn test_websocket_url_parsing() {
        assert_ok!(parse_websocket_url(
            "wss://network.my_eth_node:80/d2er2easdfasdfasdf2e"
        ));
        assert_ok!(parse_websocket_url(
            "wss://network.my_eth_node:80/<secret_key>"
        ));
        assert_ok!(parse_websocket_url(
            "wss://network.my_eth_node/<secret_key>"
        ));
        assert_ok!(parse_websocket_url("ws://network.my_eth_node/<secret_key>"));
        assert_ok!(parse_websocket_url("wss://network.my_eth_node"));
        assert!(parse_websocket_url(
            "https://mainnet.infura.io/v3/3afd67225fe34be7b185442fab14a4ba"
        )
        .is_err());
        assert!(parse_websocket_url("").is_err());
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
            state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
            state_chain_signing_key_file: Some("signing_key_file".to_owned()),
            eth_from_block: Some(1234),
            eth_node_endpoint: Some("ws://endpoint:4321".to_owned()),
            eth_stake_manager_eth_address: Some(
                web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap(),
            ),
            eth_key_manager_eth_address: Some(
                web3::types::H160::from_str("0x73d669c173d88ccb01f6daab3a3304af7a1b22c1").unwrap(),
            ),
            eth_private_key_file: Some(PathBuf::from_str("not/a/real/path.toml").unwrap()),
            health_check_hostname: Some("health_check_hostname".to_owned()),
            health_check_port: Some(1337),
            signing_db_file: Some(PathBuf::from_str("also/not/real.db").unwrap()),
        };

        // Load the junk opts into the settings
        let settings = Settings::new(opts.clone()).unwrap();

        // Compare the opts and the settings
        assert_eq!(
            opts.state_chain_ws_endpoint.unwrap(),
            settings.state_chain.ws_endpoint
        );
        assert_eq!(
            opts.state_chain_signing_key_file.unwrap(),
            settings.state_chain.signing_key_file
        );

        assert_eq!(opts.eth_from_block.unwrap(), settings.eth.from_block);
        assert_eq!(opts.eth_node_endpoint.unwrap(), settings.eth.node_endpoint);
        assert_eq!(
            opts.eth_stake_manager_eth_address.unwrap(),
            settings.eth.stake_manager_eth_address
        );
        assert_eq!(
            opts.eth_key_manager_eth_address.unwrap(),
            settings.eth.key_manager_eth_address
        );
        assert_eq!(
            opts.eth_private_key_file.unwrap(),
            settings.eth.private_key_file
        );

        assert_eq!(
            opts.health_check_hostname.unwrap(),
            settings.health_check.hostname
        );
        assert_eq!(opts.health_check_port.unwrap(), settings.health_check.port);

        assert_eq!(opts.signing_db_file.unwrap(), settings.signing.db_file);
    }
}
