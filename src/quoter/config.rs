use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

lazy_static! {
    /// The quoter config
    pub static ref QUOTER_CONFIG: QuoterConfig = match QuoterConfig::new("config/quoter") {
        Ok(config) => config,
        Err(err) => {
            error!("Failed to load config: {}", err);
            panic!("Failed to load config");
        }
    };
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for database
pub struct Database {
    /// RPC specific config
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for quoter
pub struct QuoterConfig {
    /// Oxen config
    pub database: Database,
    /// The vault node url
    pub vault_node_url: String,
}

impl QuoterConfig {
    fn new(path: &str) -> Result<Self, ConfigError> {
        let mut config = Config::new();

        // Start off by merging in the "default" configuration file
        config.merge(File::with_name(&format!("{}/default", path)))?;

        // Add in the current environment file
        // Default to 'development' env
        // Note that this file is _optional_
        let env = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());
        config.merge(File::with_name(&format!("{}/{}", path, env)).required(false))?;

        // Add in a local configuration file
        // This file shouldn't be checked in to git
        config.merge(File::with_name(&format!("{}/local", path)).required(false))?;

        // Add in settings from the environment (with a prefix of QUOTER)
        // Eg.. `QUOTER_DEBUG=1 ./target/app` would set the `debug` key
        config.merge(Environment::with_prefix("quoter"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        config.try_into()
    }
}
