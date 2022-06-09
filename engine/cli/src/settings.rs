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
    #[clap(about = "Rotate your session keys")]
    Rotate {},
    #[clap(about = "Retire from Auction participation")]
    Retire {},
    #[clap(about = "Activates an account for all future Auctions")]
    Activate {},
    #[clap(about = "Submit a query to the State Chain")]
    Query {
        #[clap(help = "Block hash to be queried")]
        block_hash: state_chain_runtime::Hash,
    },
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
        let mut cli_settings = CLISettings::default();

        // check we have all the cli args. If we do, don't bother reading from the config file + env
        let all_cl_args_set = opts.state_chain_opts.state_chain_ws_endpoint.is_some()
            && opts.state_chain_opts.state_chain_signing_key_file.is_some()
            // eth options present
            && opts.eth_opts.eth_ws_node_endpoint.is_some()
            && opts.eth_opts.eth_http_node_endpoint.is_some()
            && opts.eth_opts.eth_private_key_file.is_some();

        if !all_cl_args_set {
            cli_settings = Self::from_file_and_env(
                match &opts.config_path.clone() {
                    Some(path) => path,
                    None => "./engine/config/Default.toml",
                },
                opts,
            )?;
        }

        Ok(cli_settings)
    }

    fn from_file_and_env(file: &str, opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
        // Load the settings from the file and deserialize (and thus freeze) the entire config
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

        // Make sure the settings are clean
        cli_settings.validate_settings()?;

        Ok(cli_settings)
    }
}
