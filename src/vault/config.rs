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

#[derive(Debug, Deserialize, Copy, Clone)]
/// Defines network type for all chains each chain implementation can use
/// this type to match on network type specific actions
pub enum NetType {
    /// Mainnet, real money here
    Mainnet,
    /// Testnet, use the testing network of each chain
    Testnet,
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
    /// Main wallet address. There should be a better way of doing this, but for now this will be quickest
    pub wallet_address: String,
}

#[derive(Debug, Deserialize, Clone)]
/// Configutation for ethereum
pub struct EthConfig {
    /// The seed to derive wallets from
    pub master_root_key: String,
    /// Web3 api
    pub provider_url: String,
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for Bitcoin
pub struct BtcConfig {
    /// The seed to derive wallets from
    pub master_root_key: String,

    /// Local port the RPC enabled daemon is running on
    pub rpc_port: u16,

    /// User for authenticating to the RPC
    pub rpc_user: String,

    /// Password for authenticating to the RPC
    pub rpc_password: String,
}

#[derive(Debug, Deserialize, Clone)]
/// Configuration for vault nodes
pub struct VaultConfig {
    /// Which network type to use for all the vaults
    pub net_type: NetType,
    /// Loki config
    pub loki: LokiConfig,
    /// Eth config
    pub eth: EthConfig,
    /// Btc config
    pub btc: BtcConfig,
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
