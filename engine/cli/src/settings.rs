use chainflip_engine::settings::{Eth, EthSharedOptions, StateChain, StateChainOptions};
use config::{Config, ConfigError, File};
use serde::Deserialize;
use structopt::StructOpt;

#[derive(StructOpt, Clone)]
pub struct CLICommandLineOptions {
    #[structopt(short = "c", long = "config-path")]
    config_path: Option<String>,

    #[structopt(flatten)]
    state_chain_opts: StateChainOptions,

    #[structopt(flatten)]
    eth_opts: EthSharedOptions,

    #[structopt(subcommand)]
    pub cmd: CFCommand,
}

#[derive(StructOpt, Clone)]
pub enum CFCommand {
    Claim {
        #[structopt(help = "Amount to claim in FLIP")]
        amount: f64,
        #[structopt(help = "The Ethereum address you wish to claim your FLIP to")]
        eth_address: String,

        #[structopt(long = "register", hidden = true)]
        should_register_claim: bool,
    },
}

#[derive(Deserialize, Debug, Default)]
pub struct CLISettings {
    pub state_chain: StateChain,

    // NB: from_block isn't used here
    pub eth: Eth,
}

impl CLISettings {
    pub fn new(opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
        let mut cli_config = CLISettings::default();

        // check we have all the cli args. If we do, don't bother with the config file
        let all_cl_args_set = opts.state_chain_opts.state_chain_ws_endpoint.is_some()
            && opts.state_chain_opts.state_chain_signing_key_file.is_some()
            // eth options present
            && opts.eth_opts.eth_node_endpoint.is_some()
            && opts.eth_opts.eth_private_key_file.is_some();

        if !all_cl_args_set {
            cli_config = match opts.config_path {
                Some(path) => Self::from_file(&path)?,
                None => Self::from_file("./engine/config/Default.toml")?,
            }
        }

        // Override State Chain settings with the cmd line options
        if let Some(ws_endpoint) = opts.state_chain_opts.state_chain_ws_endpoint {
            cli_config.state_chain.ws_endpoint = ws_endpoint
        };
        if let Some(signing_key_file) = opts.state_chain_opts.state_chain_signing_key_file {
            cli_config.state_chain.signing_key_file = signing_key_file
        };

        // Override Eth settings
        if let Some(private_key_file) = opts.eth_opts.eth_private_key_file {
            cli_config.eth.private_key_file = private_key_file
        };

        if let Some(node_endpoint) = opts.eth_opts.eth_node_endpoint {
            cli_config.eth.node_endpoint = node_endpoint
        };

        Ok(cli_config)
    }

    pub fn from_file(file: &str) -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // merging in the configuration file
        s.merge(File::with_name(file))?;

        // You can deserialize (and thus freeze) the entire configuration as
        let s: Self = s.try_into()?;

        // make sure the settings are clean
        s.validate_settings()?;

        Ok(s)
    }

    pub fn validate_settings(&self) -> Result<(), ConfigError> {
        self.state_chain.validate_settings()
    }
}
