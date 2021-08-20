use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};

use config::{Config, ConfigError, File};
use serde::{de, Deserialize, Deserializer};
use web3::types::H160;

use crate::p2p::ValidatorId;

pub use anyhow::Result;
use regex::Regex;
use url::Url;

#[derive(Debug, Deserialize, Clone)]
pub struct MessageQueue {
    pub endpoint: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StateChain {
    pub ws_endpoint: String,
    pub signing_key_file: String,
    pub p2p_private_key_file: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
    pub from_block: u64,
    pub node_endpoint: String,

    // TODO: Into an Ethereum Address type?
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
    /// This includes my_id if I'm part of genesis validator set
    pub genesis_validator_ids: Vec<ValidatorId>,
    #[serde(deserialize_with = "deser_path")]
    pub db_file: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub message_queue: MessageQueue,
    pub state_chain: StateChain,
    pub eth: Eth,
    pub health_check: HealthCheck,
    pub signing: Signing,
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

    // use our visitor to deserialize an `ActualValue`
    deserializer.deserialize_any(PathVisitor)
}

impl Settings {
    /// New settings loaded from "config/Default.toml"
    pub fn new() -> Result<Self, ConfigError> {
        Settings::from_file("config/Default.toml")
    }

    /// Validates the formatting of some settings
    pub fn validate_settings(&self) -> Result<(), ConfigError> {
        // check the eth addresses
        is_eth_address(&hex::encode(&self.eth.key_manager_eth_address.0))
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        is_eth_address(&hex::encode(&self.eth.stake_manager_eth_address.0))
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        // check the Websocket URLs
        parse_websocket_url(&self.eth.node_endpoint)
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
fn parse_websocket_url(url: &str) -> Result<Url> {
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

/// Checks that the string is formatted as a valid ETH address
/// NB: Doesn't include the '0x' prefix
fn is_eth_address(address: &str) -> Result<()> {
    let re = Regex::new(r"^[a-fA-F0-9]{40}$").unwrap();

    match re.is_match(address) {
        true => Ok(()),
        false => Err(anyhow::Error::msg(format!(
            "Invalid Eth Address: {}",
            address
        ))),
    }
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

    use super::*;

    #[test]
    fn init_default_config() {
        let settings = Settings::new();
        let settings = settings.unwrap();

        assert_eq!(settings.message_queue.endpoint, "http://localhost:4222");
    }

    #[test]
    fn test_init_config_with_testing_config() {
        let test_settings = test_utils::new_test_settings();

        let test_settings = test_settings.unwrap();
        assert_eq!(
            test_settings.message_queue.endpoint,
            "http://localhost:4222"
        );
    }

    #[test]
    fn test_eth_address_parsing() {
        assert!(is_eth_address("EAd5De9C41543E4bAbB09f9fE4f79153c036044f").is_ok());
        assert!(is_eth_address("dBa9b6065Deb6___57BC779fF6736709ecBa3409").is_err());
        assert!(is_eth_address("").is_err());
    }

    #[test]
    fn test_websocket_url_parsing() {
        assert!(parse_websocket_url("wss://network.my_eth_node:80/d2er2easdfasdfasdf2e").is_ok());
        assert!(parse_websocket_url("wss://network.my_eth_node:80/<secret_key>").is_ok());
        assert!(parse_websocket_url("wss://network.my_eth_node/<secret_key>").is_ok());
        assert!(parse_websocket_url("ws://network.my_eth_node/<secret_key>").is_ok());
        assert!(parse_websocket_url("wss://network.my_eth_node").is_ok());
        assert!(parse_websocket_url(
            "https://mainnet.infura.io/v3/3afd67225fe34be7b185442fab14a4ba"
        )
        .is_err());
        assert!(parse_websocket_url("").is_err());
    }

    #[test]
    fn test_db_file_path_parsing() {
        assert!(is_valid_db_path(Path::new("data.db")).is_ok());
        assert!(is_valid_db_path(Path::new("/my/user/data/data.db")).is_ok());
        assert!(is_valid_db_path(Path::new("data.errdb")).is_err());
        assert!(is_valid_db_path(Path::new("thishasnoextension")).is_err());
    }
}
