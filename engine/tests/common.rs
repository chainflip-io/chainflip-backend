use config::{Config, ConfigError, File};
use serde::Deserialize;
use sp_core::H160;

pub const EVENT_STREAM_EMPTY_MESSAGE: &str = r#"
Event stream was empty.
- Have you run the setup script to deploy/run the contracts? (tests/scripts/setup.sh)
- Are you pointing to the correct contract address? (tests/config.toml)
"#;

#[derive(Debug, Deserialize, Clone)]
pub struct IntegrationTestSettings {
    pub eth: Eth,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
    pub key_manager_address: H160,
    pub stake_manager_address: H160,
}

impl IntegrationTestSettings {
    /// Load integration test settings from a TOML file
    pub fn from_file(file: &str) -> Result<Self, ConfigError> {
        let mut s = Config::new();
        s.merge(File::with_name(file))?;
        let s: Self = s.try_into()?;
        Ok(s)
    }
}
