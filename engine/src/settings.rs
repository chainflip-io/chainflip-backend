use config::{Config, ConfigError, File};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct MessageQueue {
    pub hostname: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StateChain {
    pub hostname: String,
    pub port: u16,
    pub signing_key_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
    pub hostname: String,
    pub port: u16,

    // TODO: Into an Ethereum Address type?
    pub stake_manager_eth_address: String,
    pub key_manager_eth_address: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    pub hostname: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub message_queue: MessageQueue,
    pub state_chain: StateChain,
    pub eth: Eth,
    pub health_check: HealthCheck,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // Start off by merging in the "default" configuration file
        s.merge(File::with_name("config/Default.toml"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_into()
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
        s.try_into()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn init_config() {
        let settings = Settings::new();
        assert!(settings.is_ok());

        let settings = settings.unwrap();
        assert_eq!(settings.message_queue.hostname, "localhost");
    }

    #[test]
    fn test_init_config_with_testing_config() {
        let test_settings = test_utils::new_test_settings();
        assert!(test_settings.is_ok());

        let test_settings = test_settings.unwrap();
        assert_eq!(test_settings.message_queue.hostname, "localhost");
    }
}
