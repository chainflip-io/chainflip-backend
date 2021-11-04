use chainflip_engine::settings::{StateChain, StateChainOptions};
use config::{Config, ConfigError, File};
use serde::Deserialize;
use structopt::StructOpt;

#[derive(StructOpt, Clone)]
pub struct CLICommandLineOptions {
    // Config path
    #[structopt(short = "c", long = "config-path")]
    config_path: Option<String>,

    #[structopt(flatten)]
    state_chain_opts: StateChainOptions,

    #[structopt(subcommand)]
    pub cmd: CFCommand,
}

#[derive(StructOpt, Clone)]
pub enum CFCommand {
    Claim { amount: u128, eth_address: String },
}

#[derive(Deserialize, Debug)]
pub struct CLISettings {
    pub state_chain: StateChain,
}

impl CLISettings {
    pub fn new(opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
        let mut cli_config = match opts.config_path {
            Some(path) => Self::from_file(&path)?,
            None => Self::from_file("./engine/config/Default")?,
        };

        // Override the settings with the cmd line options
        if let Some(ws_endpoint) = opts.state_chain_opts.state_chain_ws_endpoint {
            cli_config.state_chain.ws_endpoint = ws_endpoint
        };
        if let Some(signing_key_file) = opts.state_chain_opts.state_chain_signing_key_file {
            cli_config.state_chain.signing_key_file = signing_key_file
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
        Ok(self.state_chain.validate_settings()?)
    }
}
