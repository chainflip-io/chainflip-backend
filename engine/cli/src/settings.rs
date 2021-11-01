use chainflip_engine::settings::StateChain;
use config::{Config, ConfigError, File};
use serde::{de, Deserialize, Deserializer};
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct CLICommandLineOptions {
    // Config path
    #[structopt(short = "c", long = "config-path")]
    config_path: Option<String>,

    // State Chain Settings
    #[structopt(long = "state_chain.ws_endpoint")]
    state_chain_ws_endpoint: Option<String>,
    #[structopt(long = "state_chain.signing_key_file")]
    state_chain_signing_key_file: Option<String>,

    #[structopt(subcommand)]
    cmd: CFCommand,
}

#[derive(StructOpt)]
enum CFCommand {
    Claim { amount: u128, eth_address: String },
}

#[derive(Deserialize, Debug)]
pub struct CLISettings {
    state_chain: StateChain,
}

impl CLISettings {
    pub fn new(opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
        let mut cli_config = match opts.config_path {
            Some(path) => Self::from_file(&path)?,
            None => Self::from_file("./engine/config/Default")?,
        };

        // Override the settings with the cmd line options
        if let Some(opt) = opts.state_chain_ws_endpoint {
            cli_config.state_chain.ws_endpoint = opt
        };
        if let Some(opt) = opts.state_chain_signing_key_file {
            cli_config.state_chain.signing_key_file = opt
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
        // s.validate_settings()?;

        Ok(s)
    }
}

// impl CLICommandLineOptions {
//     /// Creates an empty CommandLineOptions with `None` for all fields
//     pub fn new() -> CLICommandLineOptions {
//         CLICommandLineOptions {
//             config_path: None,
//             state_chain_ws_endpoint: None,
//             state_chain_signing_key_file: None,
//             cmd: CFCommand::default(),
//         }
//     }
// }

// impl Default for CLICommandLineOptions {
//     fn default() -> Self {
//         Self::new()
//     }
// }
