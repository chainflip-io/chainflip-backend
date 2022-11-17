use api::primitives::{AccountRole, Hash};
use chainflip_api as api;
use clap::Parser;
use settings::{CLICommandLineOptions, CLISettings};

use crate::settings::CFCommand::*;
use anyhow::{anyhow, Result};
use utilities::clean_eth_address;

mod settings;

#[tokio::main]
async fn main() {
	// TODO: call this implicitly from within the API?
	api::use_chainflip_account_id_encoding();

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

	println!(
		"Connecting to state chain node at: `{}` and using private key located at: `{}`",
		cli_settings.state_chain.ws_endpoint,
		cli_settings.state_chain.signing_key_file.display()
	);

	match command_line_opts.cmd {
		Claim { amount, eth_address, should_register_claim } =>
			request_claim(amount, &eth_address, &cli_settings, should_register_claim).await,
		CheckClaim => check_claim(&cli_settings.state_chain).await,
		RegisterAccountRole { role } => register_account_role(role, &cli_settings).await,
		Rotate {} => rotate_keys(&cli_settings.state_chain).await,
		Retire {} => api::retire_account(&cli_settings.state_chain).await,
		Activate {} => api::activate_account(&cli_settings.state_chain).await,
		Query { block_hash } => request_block(block_hash, &cli_settings.state_chain).await,
		VanityName { name } => api::set_vanity_name(name, &cli_settings.state_chain).await,
		ForceRotation { id } => api::force_rotation(id, &cli_settings.state_chain).await,
	}
}

pub async fn request_block(
	block_hash: Hash,
	state_chain_settings: &settings::StateChain,
) -> Result<()> {
	match api::request_block(block_hash, state_chain_settings).await {
		Ok(block) => {
			println!("{:#?}", block);
			Ok(())
		},
		Err(err) => {
			println!("Could not find block with block hash {:x?}", block_hash);
			Err(err)
		},
	}
}

async fn register_account_role(role: AccountRole, settings: &settings::CLISettings) -> Result<()> {
	println!(
        "Submitting `register-account-role` with role: {:?}. This cannot be reversed for your account.",
        role
    );

	if !confirm_submit() {
		return Ok(())
	}

	api::register_account_role(role, &settings.state_chain).await
}

pub async fn rotate_keys(state_chain_settings: &settings::StateChain) -> Result<()> {
	let tx_hash = api::rotate_keys(state_chain_settings).await?;
	println!("Session key rotated at tx {:#x}.", tx_hash);

	Ok(())
}

async fn check_claim(state_chain_settings: &settings::StateChain) -> Result<()> {
	let certificate = api::wait_for_claim_certificate(state_chain_settings).await?;
	println!("Claim certificate found: {:?}", certificate);
	Ok(())
}

async fn request_claim(
	amount: f64,
	eth_address: &str,
	settings: &CLISettings,
	should_register_claim: bool,
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

	let atomic_amount: u128 = (amount * 10_f64.powi(18)) as u128;

	println!(
		"Submitting claim with amount `{}` FLIP (`{}` Flipperinos) to ETH address `0x{}`.",
		amount,
		atomic_amount,
		hex::encode(eth_address)
	);

	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api::request_claim(atomic_amount, eth_address, &settings.state_chain).await?;

	println!("Your claim has transaction hash: `{:#x}`. Waiting for signed claim data...", tx_hash);

	let claim_cert = api::wait_for_claim_certificate(&settings.state_chain).await?;

	println!("Your claim certificate is: {:?}", hex::encode(claim_cert.clone()));

	if should_register_claim {
		let tx_hash = api::register_claim(&settings.eth, &settings.state_chain, claim_cert)
			.await
			.expect("Failed to register claim on ETH");

		println!("Submitted claim to Ethereum successfully with tx_hash: {:#x}", tx_hash);
	} else {
		println!(
			"Your claim request has been successfully registered. Please proceed to the Staking UI to complete your claim."
		);
	}

	Ok(())
}

fn confirm_submit() -> bool {
	use std::{io, io::*};

	loop {
		print!("Do you wish to proceed? [y/n] > ");
		std::io::stdout().flush().unwrap();
		let mut input = String::new();
		io::stdin().read_line(&mut input).expect("Error: Failed to get user input");

		let input = input.trim();

		match input {
			"y" | "yes" | "1" | "true" | "ofc" => {
				println!("Submitting...");
				return true
			},
			"n" | "no" | "0" | "false" | "nah" => {
				println!("Ok, exiting...");
				return false
			},
			_ => continue,
		}
	}
}
