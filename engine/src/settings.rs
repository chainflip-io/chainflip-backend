use config::{Config, ConfigError, File};

use serde::Deserialize;

use crate::p2p::ValidatorId;

pub use anyhow::Result;
use regex::Regex;
use url::Url;

#[derive(Debug, Deserialize, Clone)]
pub struct MessageQueue {
    pub hostname: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StateChain {
    pub hostname: String,
    pub ws_port: u16,
    pub rpc_port: u16,
    pub signing_key_file: String,
    pub p2p_priv_key_file: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
    pub node_endpoint: String,

    // TODO: Into an Ethereum Address type?
    pub stake_manager_eth_address: String,
    pub key_manager_eth_address: String,
    pub private_key_file: String,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub message_queue: MessageQueue,
    pub state_chain: StateChain,
    pub eth: Eth,
    pub health_check: HealthCheck,
    pub signing: Signing,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // Start off by merging in the "default" configuration file
        s.merge(File::with_name("config/Default.toml"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        let s: Self = s.try_into()?;

        // make sure the settings are clean
        s.validate_settings()?;

        Ok(s)
    }

    /// validates the formatting of some settings
    pub fn validate_settings(&self) -> Result<(), ConfigError> {
        // check the eth addresses
        is_eth_address(&self.eth.key_manager_eth_address)
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        is_eth_address(&self.eth.stake_manager_eth_address)
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        // check the Websocket URLs
        parse_websocket_url(&self.eth.node_endpoint)
            .map_err(|e| ConfigError::Message(e.to_string()))?;

        Ok(())
    }
}

/// parse the URL and check that it is a valid websocket url
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

/// checks that the string is formatted as an eth address
fn is_eth_address(address: &str) -> Result<()> {
    let re = Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();

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

    pub fn new_test_settings() -> Result<Settings, ConfigError> {
        let mut s = Config::new();

        // Start off by merging in the "testing" configuration file
        s.merge(File::with_name("config/Testing.toml"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        let s: Settings = s.try_into()?;

        // make sure the settings are clean
        s.validate_settings()?;

        Ok(s)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn init_config() {
        let settings = Settings::new();
        let settings = settings.unwrap();

        assert_eq!(settings.message_queue.hostname, "localhost");
    }

    #[test]
    fn test_init_config_with_testing_config() {
        let test_settings = test_utils::new_test_settings();

        let test_settings = test_settings.unwrap();
        assert_eq!(test_settings.message_queue.hostname, "localhost");
    }

    #[test]
    fn test_setting_parsing() {
        // test the eth address regex parsing
        assert!(is_eth_address("0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f").is_ok());
        assert!(is_eth_address("0xdBa9b6065Deb6___57BC779fF6736709ecBa3409").is_err());
        assert!(is_eth_address("EAd5De9C41543E4bAbB09f9fE4f79153c036044f").is_err());
        assert!(is_eth_address("").is_err());

        // test the websocket parsing
        assert!(parse_websocket_url("wss://network.my_eth_node:80/<secret_key>").is_ok());
        assert!(parse_websocket_url("wss://network.my_eth_node/<secret_key>").is_ok());
        assert!(parse_websocket_url("wss://network.my_eth_node").is_ok());
        assert!(parse_websocket_url(
            "https://mainnet.infura.io/v3/3afd67225fe34be7b185442fab14a4ba"
        )
        .is_err());
        assert!(parse_websocket_url("").is_err());
    }
}
