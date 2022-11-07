use chainflip_api as api;
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use settings::{CLICommandLineOptions, CLISettings};

use crate::settings::CFCommand::*;
use anyhow::{anyhow, Result};
use utilities::clean_eth_address;

mod settings;

#[tokio::main]
async fn main() {
	// TODO: move this to API
	use_chainflip_account_id_encoding();

	std::process::exit(match run_cli().await {
		Ok(_) => 0,
		Err(err) => {
			eprintln!("Error: {:?}", err);
			1
		},
	})
}

async fn run_cli() -> Result<()> {
	let command_line_opts = CLICommandLineOptions::parse();
	let cli_settings = CLISettings::new(command_line_opts.clone()).map_err(|err| anyhow!("Please ensure your config file path is configured correctly and the file is valid. You can also just set all configurations required command line arguments.\n{}", err))?;

	let logger = chainflip_engine::logging::utils::new_discard_logger();

	println!(
		"Connecting to state chain node at: `{}` and using private key located at: `{}`",
		cli_settings.state_chain.ws_endpoint,
		cli_settings.state_chain.signing_key_file.display()
	);

	match command_line_opts.cmd {
		Claim { amount, eth_address, should_register_claim } =>
			request_claim(amount, &eth_address, &cli_settings, should_register_claim, &logger).await,
		RegisterAccountRole { role } =>
			api::register_account_role(role, &cli_settings.state_chain, &logger).await,
		Rotate {} => api::rotate_keys(&cli_settings.state_chain, &logger).await,
		Retire {} => api::retire_account(&cli_settings.state_chain, &logger).await,
		Activate {} => api::activate_account(&cli_settings.state_chain, &logger).await,
		Query { block_hash } => api::request_block(block_hash, &cli_settings.state_chain).await,
		VanityName { name } => api::set_vanity_name(name, &cli_settings.state_chain, &logger).await,
		ForceRotation { id } => api::force_rotation(id, &cli_settings.state_chain, &logger).await,
	}
}

async fn request_claim(
	amount: f64,
	eth_address: &str,
	settings: &CLISettings,
	should_register_claim: bool,
	logger: &slog::Logger,
) -> Result<()> {
	// Sanitise data

	let eth_address = clean_eth_address(eth_address)
		.map_err(|error| anyhow!("You supplied an invalid ETH address: {}", error))
		.and_then(|eth_address|
			if eth_address == [0; 20] {
				Err(anyhow!("Cannot submit claim to the zero address. If you really want to do this, use 0x000000000000000000000000000000000000dead instead."))
			} else {
				Ok(eth_address)
			}
		)?;

	api::request_claim(
		amount,
		eth_address,
		&settings.state_chain,
		&settings.eth,
		should_register_claim,
		logger,
	)
	.await
}
