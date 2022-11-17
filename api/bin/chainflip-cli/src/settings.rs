use chainflip_api::primitives::{AccountRole, Hash, ProposalId};
use chainflip_engine::settings::{CfSettings, Eth, EthOptions, StateChain, StateChainOptions};
use clap::Parser;
use config::{ConfigError, Source, Value};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Parser, Clone, Debug)]
pub struct CLICommandLineOptions {
	#[clap(short = 'c', long = "config-path")]
	config_path: Option<String>,

	#[clap(flatten)]
	state_chain_opts: StateChainOptions,

	#[clap(flatten)]
	eth_opts: EthOptions,

	#[clap(subcommand)]
	pub cmd: CFCommand,
}

impl Source for CLICommandLineOptions {
	fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> std::result::Result<config::Map<String, Value>, ConfigError> {
		let mut map: HashMap<String, Value> = HashMap::new();

		self.state_chain_opts.insert_all(&mut map);

		self.eth_opts.insert_all(&mut map);

		Ok(map)
	}
}

#[cfg(test)]
impl Default for CLICommandLineOptions {
	fn default() -> Self {
		Self {
			config_path: None,
			state_chain_opts: StateChainOptions::default(),
			eth_opts: EthOptions::default(),
			// an arbitrary simple command
			cmd: CFCommand::Retire {},
		}
	}
}

#[derive(Parser, Clone, Debug)]
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
		#[clap(help = "Validator (v), Liquidity Provider (lp), Relayer (r)", value_parser = account_role_parser)]
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
		block_hash: Hash,
	},
	#[clap(
        // this is only useful for testing. No need to show to the end user.
        hide = true,
        about = "Force a key rotation. This can only be executed by the governance dictator"
    )]
	ForceRotation {
		#[clap(help = "The governance proposal id that will be associated with this rotation.")]
		id: ProposalId,
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
	type CommandLineOptions = CLICommandLineOptions;

	fn validate_settings(&self) -> Result<(), ConfigError> {
		self.eth.validate_settings()?;

		self.state_chain.validate_settings()
	}
}

impl CLISettings {
	/// New settings loaded from the `config_path` in the `CommandLineOptions` or
	/// "config/Default.toml" if none, with overridden values from the environment and
	/// `CommandLineOptions`
	pub fn new(opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
		Self::load_settings_from_all_sources(
			"",
			"config/Default.toml",
			opts.config_path.clone(),
			opts,
		)
	}
}

#[cfg(test)]
mod tests {

	use std::{path::PathBuf, str::FromStr};

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

		let settings = CLISettings::load_settings_from_all_sources(
			"",
			"../config/Default.toml",
			None,
			CLICommandLineOptions::default(),
		)
		.unwrap();

		assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
		assert_eq!(settings.eth.ws_node_endpoint, "ws://localhost:8545");
	}

	#[test]
	fn test_all_command_line_options() {
		// Fill the command line options with test data that is different for that in `Default.toml`
		let opts = CLICommandLineOptions {
			config_path: None, // Not used in this test

			state_chain_opts: StateChainOptions {
				state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
				state_chain_signing_key_file: Some(PathBuf::from_str("signing_key_file").unwrap()),
			},

			eth_opts: EthOptions {
				eth_ws_node_endpoint: Some("ws://endpoint2:1234".to_owned()),
				eth_http_node_endpoint: Some("http://endpoint3:1234".to_owned()),
				eth_private_key_file: Some(PathBuf::from_str("eth_key_file").unwrap()),
			},

			cmd: CFCommand::Rotate {}, // Not used in this test
		};

		// Load the test opts into the settings
		let settings = CLISettings::new(opts.clone()).unwrap();

		// Compare the opts and the settings
		assert_eq!(
			opts.state_chain_opts.state_chain_ws_endpoint.unwrap(),
			settings.state_chain.ws_endpoint
		);
		assert_eq!(
			opts.state_chain_opts.state_chain_signing_key_file.unwrap(),
			settings.state_chain.signing_key_file
		);
		assert_eq!(opts.eth_opts.eth_ws_node_endpoint.unwrap(), settings.eth.ws_node_endpoint);
		assert_eq!(opts.eth_opts.eth_http_node_endpoint.unwrap(), settings.eth.http_node_endpoint);
		assert_eq!(opts.eth_opts.eth_private_key_file.unwrap(), settings.eth.private_key_file);
	}
}
