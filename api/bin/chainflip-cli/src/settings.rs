// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use chainflip_api::{
	primitives::{state_chain_runtime, Asset, EpochIndex, ForeignChain},
	AddressString, BasisPoints,
};
pub use chainflip_engine::settings::StateChain;
use chainflip_engine::{
	constants::{CONFIG_ROOT, DEFAULT_CONFIG_ROOT},
	settings::{
		resolve_settings_path, CfSettings, PathResolutionExpectation, StateChainOptions,
		DEFAULT_SETTINGS_DIR,
	},
};
use clap::Parser;
use config::{ConfigBuilder, ConfigError, Source, Value};
use serde::Deserialize;
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
};

#[derive(Parser, Clone, Debug)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"))]
pub struct CLICommandLineOptions {
	// Specifying a config root implies the existence of a Settings.toml file
	#[clap(short = 'c', long = "config-root", env = CONFIG_ROOT, default_value = DEFAULT_CONFIG_ROOT, help = "Specifying a config root implies the existence of a Settings.toml file there.")]
	pub config_root: String,

	#[clap(flatten)]
	state_chain_opts: StateChainOptions,

	#[clap(subcommand)]
	pub cmd: CliCommand,
}

impl Source for CLICommandLineOptions {
	fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> std::result::Result<config::Map<String, Value>, ConfigError> {
		let mut map: HashMap<String, Value> = HashMap::new();

		self.state_chain_opts.insert_all(&mut map);

		Ok(map)
	}
}

#[cfg(test)]
impl Default for CLICommandLineOptions {
	fn default() -> Self {
		Self {
			config_root: DEFAULT_CONFIG_ROOT.to_owned(),
			state_chain_opts: StateChainOptions::default(),
			// an arbitrary simple command
			cmd: CliCommand::StopBidding {},
		}
	}
}

#[derive(Parser, Clone, Debug)]
pub struct SwapRequestParams {
	/// Source asset ("ETH"|"DOT")
	pub source_asset: Asset,
	/// Egress asset ("ETH"|"DOT")
	pub destination_asset: Asset,
	// Note: we delay parsing this into `ForeignChainAddress`
	// until we know which kind of address to expect (based
	// on destination_asset)
	/// Egress asset address to receive funds after the swap
	pub destination_address: AddressString,
	/// Commission to the broker in basis points
	pub broker_commission: BasisPoints,
	/// Commission to the booster in basis points
	pub boost_fee: Option<u16>,
}
#[derive(Parser, Clone, Debug)]
pub struct WithdrawFeesParams {
	/// Asset to withdraw ("ETH"|"DOT")
	pub asset: Asset,
	/// Egress asset address to receive withdrawn funds
	pub destination_address: AddressString,
}
#[derive(clap::Subcommand, Clone, Debug)]
pub enum BrokerSubcommands {
	WithdrawFees(WithdrawFeesParams),
	/// Register this account as a broker account.
	RegisterAccount,
	/// De-register this broker account.
	DeregisterAccount,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum LiquidityProviderSubcommands {
	/// Request a liquidity deposit address.
	RequestLiquidityDepositAddress {
		/// Asset to deposit ("ETH"|"DOT")
		asset: Asset,
		boost_fee: Option<u16>,
	},
	/// Register a Liquidity Refund Address for the given chain. An address must be
	/// registered to request a deposit address for the given chain.
	RegisterLiquidityRefundAddress { chain: ForeignChain, address: AddressString },
	/// Register this account as a liquidity provider account.
	RegisterAccount,
	/// De-register this liquidity provider account.
	DeregisterAccount,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum ValidatorSubcommands {
	/// Register this account as a validator account.
	RegisterAccount,
	/// De-register this validator account.
	DeregisterAccount,
	/// Start bidding to participate in all future auctions.
	StartBidding,
	/// Stop bidding, thereby stopping participation in auctions.
	StopBidding,
	/// Accept an operator's claim to manage this validator.
	AcceptOperator {
		/// The operator account ID to accept
		operator_id: String,
	},
	/// Remove the operator from managing this validator.
	RemoveOperator,
}

#[derive(Parser, Clone, Debug)]
pub enum CliCommand {
	/// Broker specific commands
	#[clap(subcommand)]
	Broker(BrokerSubcommands),
	/// Liquidity provider specific commands
	#[clap(subcommand, name = "lp")]
	LiquidityProvider(LiquidityProviderSubcommands),
	/// Validator specific commands
	#[clap(subcommand)]
	Validator(ValidatorSubcommands),
	#[clap(
		about = "Request a redemption. After requesting the redemption, please proceed to theAuctions App to complete the redeeming process."
	)]
	Redeem {
		#[clap(
			help = "Amount to redeem in FLIP (omit this option to redeem all available FLIP). Up to 6 decimal places, any more are rounded.",
			long = "exact"
		)]
		amount: Option<f64>,
		#[clap(help = "The Ethereum address you wish to redeem your FLIP to.")]
		eth_address: String,
		#[clap(
			help = "Optional executor address. If specified, only this address will be able to execute the redemption."
		)]
		executor_address: Option<String>,
	},
	#[clap(about = "Rebalance FLIP by transferring it to another account.")]
	Rebalance {
		#[clap(
			help = "Amount to transfer in FLIP (omit this option to redeem all available FLIP). Up to 6 decimal places, any more are rounded.",
			long = "exact"
		)]
		amount: Option<f64>,
		#[clap(help = "The State Chain account ID of the recipient.")]
		recipient_account_id: String,
		#[clap(
			help = "An optional Ethereum address under which restriction conditions we transfer the FLIP.",
			long = "restricted-address"
		)]
		restricted_address: Option<String>,
	},
	#[clap(
		about = "Irreversible action that restricts your account to only be able to redeem to the specified address"
	)]
	BindRedeemAddress {
		#[clap(help = "The Ethereum address you wish to bind your account to")]
		eth_address: String,
	},
	#[clap(
		about = "Irreversible action that restricts your account to only be able to execute registered redemptions with the specified address"
	)]
	BindExecutorAddress {
		#[clap(help = "The Ethereum address you wish to bind your account to")]
		eth_address: String,
	},
	#[clap(about = "Shows the redeem address your account is bound to")]
	GetBoundRedeemAddress,
	#[clap(about = "Shows the executor address your account is bound to")]
	GetBoundExecutorAddress,
	#[clap(about = "Rotate your session keys")]
	Rotate {},
	#[clap(
		about = "Stop bidding, thereby stop participating in auctions. [DEPRECATED - use 'validator stop-bidding' instead]"
	)]
	StopBidding {},
	#[clap(
		about = "The account starts bidding for all future auctions, until it stops bidding. [DEPRECATED - use 'validator start-bidding' instead]"
	)]
	StartBidding {},
	#[clap(about = "Set a UTF-8 vanity name for your node (max length 64)")]
	VanityName {
		#[clap(help = "Name in UTF-8 (max length 64)")]
		name: String,
	},
	#[clap(about = "Check if it is safe to update your node/engine")]
	PreUpdateCheck {},
	#[clap(
        // This is only useful for testing. No need to show to the end user.
        hide = true,
        about = "Force a key rotation. This can only be executed by the governance dictator"
    )]
	ForceRotation {},
	/// Generates the private/public keys required needed to run a Chainflip validator node. These
	/// are the Node Key, Ethereum Key and Validator Key. The Validator Key and Ethereum Key can be
	/// recovered using the seed phrase. The Node Key does not control any funds and therefore
	/// doesn't need to be recoverable. It is generated independently of the seed phrase.
	///
	/// Note the seed phrase can only be used to recover keys using this utility. Notably, it isn't
	/// possible to use the seed phrase to import the Ethereum wallet into Metamask. This is by
	/// design: the Ethereum wallet should remain for the exclusive use of the Validator node.
	GenerateKeys {
		/// Output to the cmd line as JSON instead of pretty-printing the keys.
		#[clap(short, long, action)]
		json: bool,
		/// Provide a path to a directory where the keys will be saved.
		#[clap(short, long, action)]
		path: Option<PathBuf>,
		/// Supply a seed to generate the keys deterministically (restore keys).
		#[clap(short, long, action)]
		seed_phrase: Option<String>,
	},
	#[clap(about = "Count how many validators witnessed a given callhash")]
	CountWitnesses {
		#[clap(help = "The hash representing the call to check")]
		hash: state_chain_runtime::Hash,
		#[clap(help = "The epoch to check, default to the current one")]
		epoch_index: Option<EpochIndex>,
	},
}

#[derive(Deserialize, Debug, Default)]
pub struct CLISettings {
	pub state_chain: StateChain,
}

impl CfSettings for CLISettings {
	type CommandLineOptions = CLICommandLineOptions;

	fn validate_settings(&mut self, config_root: &Path) -> Result<(), ConfigError> {
		self.state_chain.validate_settings()?;
		self.state_chain.signing_key_file = resolve_settings_path(
			config_root,
			&self.state_chain.signing_key_file,
			Some(PathResolutionExpectation::ExistingFile),
		)?;

		Ok(())
	}

	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
		config_root: &str,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		config_builder
			.set_default(
				"state_chain.signing_key_file",
				PathBuf::from(config_root)
					.join("keys/signing_key_file")
					.to_str()
					.expect("Invalid signing_key_file path"),
			)?
			.set_default(
				"eth.private_key_file",
				PathBuf::from(config_root)
					.join("keys/eth_private_key")
					.to_str()
					.expect("Invalid eth_private_key path"),
			)
	}
}

impl CLISettings {
	/// New settings loaded from "$base_config_path/config/Settings.toml",
	/// environment and `CommandLineOptions`
	pub fn new(opts: CLICommandLineOptions) -> Result<Self, ConfigError> {
		Self::load_settings_from_all_sources(opts.config_root.clone(), DEFAULT_SETTINGS_DIR, opts)
	}
}

#[cfg(test)]
mod tests {

	use std::{path::PathBuf, str::FromStr};

	use super::*;

	use chainflip_engine::constants::{ETH_HTTP_ENDPOINT, ETH_WS_ENDPOINT};

	pub fn set_test_env() {
		use std::env;

		env::set_var(ETH_HTTP_ENDPOINT, "http://localhost:8545");
		env::set_var(ETH_WS_ENDPOINT, "ws://localhost:8545");
	}

	#[test]
	#[ignore = "Requires config file at root"]
	fn init_default_config() {
		set_test_env();

		let settings = CLISettings::load_settings_from_all_sources(
			DEFAULT_CONFIG_ROOT.to_owned(),
			DEFAULT_SETTINGS_DIR,
			CLICommandLineOptions::default(),
		)
		.unwrap();

		assert_eq!(settings.state_chain.ws_endpoint, "ws://localhost:9944");
	}

	#[test]
	#[ignore = "Requires config file at default root"]
	fn test_all_command_line_options() {
		// Fill the options with test values that will pass the parsing/validation.
		// The test values need to be different from the default values set during `set_defaults()`
		// for the test to work. `config_root` and `cmd` are not used in this test because they are
		// not settings.
		let opts = CLICommandLineOptions {
			config_root: CLICommandLineOptions::default().config_root,

			state_chain_opts: StateChainOptions {
				state_chain_ws_endpoint: Some("ws://endpoint:1234".to_owned()),
				state_chain_signing_key_file: Some(PathBuf::from_str("signing_key_file").unwrap()),
			},

			cmd: CliCommand::Rotate {}, // Not used in this test
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
	}
}
