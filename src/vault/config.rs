use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

lazy_static! {
    /// The vault node config
    pub static ref VAULT_CONFIG: VaultConfig = match VaultConfig::new("config/vault") {
        Ok(config) => config,
        Err(err) => {
            error!("Failed to load config: {}", err);
            panic!("Failed to load config");
        }
    };
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for the loki wallet rpc
pub struct LokiRpcConfig {
    /// The port that the wallet rpc is running on
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for loki
pub struct LokiConfig {
    /// RPC specific config
    pub rpc: LokiRpcConfig,
}

#[derive(Debug, Deserialize, Clone)]
/// Configutation for ethereum
pub struct EthConfig {
    /// The seed to derive wallets from
    pub master_root_key: String,
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for vault nodes
pub struct VaultConfig {
    /// Loki config
    pub loki: LokiConfig,
    /// Eth config
    pub eth: EthConfig,
}

impl VaultConfig {
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

        // Add in settings from the environment (with a prefix of VAULT)
        // Eg.. `VAULT_DEBUG=1 ./target/app` would set the `debug` key
        config.merge(Environment::with_prefix("vault"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        config.try_into()
    }
}
