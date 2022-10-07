use cf_primitives::AccountRole;
use chainflip_engine::settings::{
    CfSettings, Eth, EthSharedOptions, StateChain, StateChainOptions,
};
use clap::Parser;
use config::ConfigError;
use serde::Deserialize;

#[derive(Parser, Clone)]
pub struct CLICommandLineOptions {
    #[clap(short = 'c', long = "config-path")]
    config_path: Option<String>,

    #[clap(flatten)]
    state_chain_opts: StateChainOptions,

    #[clap(flatten)]
    eth_opts: EthSharedOptions,

    #[clap(subcommand)]
    pub cmd: CFCommand,
}

impl CLICommandLineOptions {
    pub fn all_options_are_set(&self) -> bool {
        self.state_chain_opts.state_chain_ws_endpoint.is_some()
            && self.state_chain_opts.state_chain_signing_key_file.is_some()
            // eth options present
            && self.eth_opts.eth_ws_node_endpoint.is_some()
            && self.eth_opts.eth_http_node_endpoint.is_some()
            && self.eth_opts.eth_private_key_file.is_some()
    }
}

#[cfg(test)]
impl Default for CLICommandLineOptions {
    fn default() -> Self {
        Self {
            config_path: None,
            state_chain_opts: StateChainOptions::default(),
            eth_opts: EthSharedOptions::default(),
            // an arbitrary simple command
            cmd: CFCommand::Retire {},
        }
    }
}

#[derive(Parser, Clone)]
pub enum CFCommand {
    #[clap(about = "Submit an extrinsic to request generation of a claim certificate")]
    Claim {
        #[clap(help = "Amount to claim in FLIP")]
        amount: f64,
        #[clap(help = "The Ethereum address you wish to claim your FLIP to")]
        eth_address: String,

        #[clap(long = "register", hide = true)]
        should_register_claim: bool,
    },
    #[clap(about = "Set your account role to the Validator, Relayer, Liquidity Provider")]
    RegisterAccountRole {
        #[clap(value_parser = account_role_parser)]
        role: AccountRole,
    },
    #[clap(about = "Rotate your session keys")]
    Rotate {},
    #[clap(about = "Retire from Auction participation")]
    Retire {},
    #[clap(about = "Activates an account for all future Auctions")]
    Activate {},
    #[clap(about = "Set a UTF-8 vanity name for your node (max length 64)")]
    VanityName {
        #[clap(help = "Name in UTF-8 (max length 64)")]
        name: String,
    },
    #[clap(about = "Submit a query to the State Chain")]
    Query {
        #[clap(help = "Block hash to be queried")]
        block_hash: state_chain_runtime::Hash,
    },
}

fn account_role_parser(s: &str) -> Result<AccountRole, String> {
    let lower_str = s.to_lowercase();
    if lower_str == "v" || lower_str == "validator" {
        Ok(AccountRole::Validator)
    } else if lower_str == "lp" || lower_str == "liquidity provider" {
        Ok(AccountRole::LiquidityProvider)
    } else if lower_str == "r" || lower_str == "relayer" {
        Ok(AccountRole::Relayer)
    } else {
        Err(format!("{} is not a valid role. The valid roles (with their shorthand input) are: 'Validator' (v), 'Liquidity Provider' (lp), 'Relayer' (r)", s))
    }
}

#[derive(Deserialize, Debug, Default)]
pub struct CLISettings {
    pub state_chain: StateChain,

    pub eth: Eth,
}

impl CfSettings for CLISettings {
    type Settings = Self;

    fn validate_settings(&self) -> Result<(), ConfigError> {
        self.eth.validate_settings()?;

        self.state_chain.validate_settings()
    }
}

impl CLISettings {
    pub fn new(opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
        let cli_settings = if !opts.all_options_are_set() {
            Self::from_file_and_env(
                match &opts.config_path.clone() {
                    Some(path) => path,
                    None => "./engine/config/Default.toml",
                },
                opts,
            )?
        } else {
            CLISettings::default()
        };

        cli_settings.validate_settings()?;

        Ok(cli_settings)
    }

    fn from_file_and_env(file: &str, opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
        let mut cli_settings = Self::settings_from_file_and_env(file)?;

        // Override State Chain settings with the cmd line options
        if let Some(ws_endpoint) = opts.state_chain_opts.state_chain_ws_endpoint {
            cli_settings.state_chain.ws_endpoint = ws_endpoint
        };
        if let Some(signing_key_file) = opts.state_chain_opts.state_chain_signing_key_file {
            cli_settings.state_chain.signing_key_file = signing_key_file
        };

        // Override Eth settings
        if let Some(private_key_file) = opts.eth_opts.eth_private_key_file {
            cli_settings.eth.private_key_file = private_key_file
        };

        if let Some(ws_node_endpoint) = opts.eth_opts.eth_ws_node_endpoint {
            cli_settings.eth.ws_node_endpoint = ws_node_endpoint
        };

        if let Some(http_node_endpoint) = opts.eth_opts.eth_http_node_endpoint {
            cli_settings.eth.http_node_endpoint = http_node_endpoint
        };

        Ok(cli_settings)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use chainflip_engine::constants::{ETH_HTTP_NODE_ENDPOINT, ETH_WS_NODE_ENDPOINT};

    pub fn set_test_env() {
        use std::env;

        env::set_var(ETH_HTTP_NODE_ENDPOINT, "http://localhost:8545");
        env::set_var(ETH_WS_NODE_ENDPOINT, "ws://localhost:8545");
    }

    #[test]
    fn init_default_config() {
        set_test_env();

        let settings = CLISettings::from_file_and_env(
            "../config/Default.toml",
            CLICommandLineOptions::default(),
        )
        .unwrap();

        assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
        assert_eq!(settings.eth.ws_node_endpoint, "ws://localhost:8545");
    }
}
