use std::path::PathBuf;

use api::primitives::{AccountRole, ClaimAmount, Hash};
use chainflip_api as api;
use clap::Parser;
use settings::{CLICommandLineOptions, CLISettings};

#[cfg(feature = "ibiza")]
use crate::settings::{LiquidityProviderSubcommands, RelayerSubcommands};
#[cfg(feature = "ibiza")]
use api::primitives::Asset;

use crate::settings::{Claim, CliCommand::*};
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
		#[cfg(feature = "ibiza")]
		Relayer(RelayerSubcommands::SwapIntent(params)) =>
			swap_intent(&cli_settings.state_chain, params).await,
		#[cfg(feature = "ibiza")]
		LiquidityProvider(LiquidityProviderSubcommands::Deposit { asset }) =>
			liquidity_deposit(&cli_settings.state_chain, asset).await,
		Claim(Claim::Request { amount, eth_address, should_register_claim }) =>
			request_claim(amount, &eth_address, &cli_settings, should_register_claim).await,
		Claim(Claim::Check {}) => check_claim(&cli_settings.state_chain).await,
		RegisterAccountRole { role } => register_account_role(role, &cli_settings).await,
		Rotate {} => rotate_keys(&cli_settings.state_chain).await,
		Retire {} => api::retire_account(&cli_settings.state_chain).await,
		Activate {} => api::activate_account(&cli_settings.state_chain).await,
		Query { block_hash } => request_block(block_hash, &cli_settings.state_chain).await,
		VanityName { name } => api::set_vanity_name(name, &cli_settings.state_chain).await,
		ForceRotation { id } => api::force_rotation(id, &cli_settings.state_chain).await,
		GenerateKeys {} => generate_keys(),
	}
}

#[cfg(feature = "ibiza")]
pub async fn swap_intent(
	state_chain_settings: &settings::StateChain,
	params: settings::SwapIntentParams,
) -> Result<()> {
	use api::primitives::{ForeignChain, ForeignChainAddress};
	use utilities::clean_dot_address;

	let egress_address = match ForeignChain::from(params.egress_asset) {
		ForeignChain::Ethereum => {
			let addr = clean_eth_address(&params.egress_address)
				.map_err(|err| anyhow!("Failed to parse address: {}", err))?;
			ForeignChainAddress::Eth(addr)
		},
		ForeignChain::Polkadot => {
			let addr = clean_dot_address(&params.egress_address)
				.map_err(|err| anyhow!("Failed to parse address: {}", err))?;
			ForeignChainAddress::Dot(addr)
		},
	};

	let address = api::register_swap_intent(
		state_chain_settings,
		params.ingress_asset,
		params.egress_asset,
		egress_address,
		params.relayer_commission,
	)
	.await?;
	println!("Ingress address: {}", address);
	Ok(())
}

#[cfg(feature = "ibiza")]
pub async fn liquidity_deposit(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<()> {
	let address = api::liquidity_deposit(state_chain_settings, asset).await?;
	println!("Ingress address: {}", address);
	Ok(())
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
	const POLL_LIMIT_BLOCKS: usize = 10;

	if let Some(certificate) =
		api::poll_for_claim_certificate(state_chain_settings, POLL_LIMIT_BLOCKS).await?
	{
		println!("Claim certificate found: {:?}", hex::encode(certificate));
	} else {
		println!("No claim certificate found. Try again later.");
	}
	Ok(())
}

async fn request_claim(
	amount: Option<f64>,
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

	let amount = match amount {
		Some(amount_float) => {
			let atomic_amount = (amount_float * 10_f64.powi(18)) as u128;

			println!(
				"Submitting claim with amount `{}` FLIP (`{}` Flipperinos) to ETH address `0x{}`.",
				amount_float,
				atomic_amount,
				hex::encode(eth_address)
			);

			ClaimAmount::Exact(atomic_amount)
		},
		None => {
			println!(
				"Submitting claim with MAX amount to ETH address `0x{}`.",
				hex::encode(eth_address)
			);

			ClaimAmount::Max
		},
	};

	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api::request_claim(amount, eth_address, &settings.state_chain).await?;

	println!("Your claim has transaction hash: `{:#x}`. Waiting for signed claim data...", tx_hash);

	const POLL_LIMIT_BLOCKS: usize = 20;

	match api::poll_for_claim_certificate(&settings.state_chain, POLL_LIMIT_BLOCKS).await? {
		Some(claim_cert) => {
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
		},
		None => {
			println!("Certificate takes longer to generate than expected. Please check claim certificate later.")
		},
	};

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

fn generate_keys() -> Result<()> {
	use std::fs;

	// TODO: Support the custom key path/file settings.
	const NODE_KEY_FILE_NAME: &str = "node_key_file";
	const SIGNING_KEY_FILE_NAME: &str = "signing_key_file";
	const ETHEREUM_KEY_FILE_NAME: &str = "ethereum_key_file";
	const OUTPUT_PATH: &str = "/etc/chainflip/keys";

	let output_path = PathBuf::from(OUTPUT_PATH);

	if output_path.is_file() {
		anyhow::bail!("Invalid keys path {}", output_path.to_string_lossy());
	}
	if !output_path.exists() {
		std::fs::create_dir_all(output_path.clone())?
	}
	if output_path.join(NODE_KEY_FILE_NAME).exists() ||
		output_path.join(SIGNING_KEY_FILE_NAME).exists() ||
		output_path.join(ETHEREUM_KEY_FILE_NAME).exists()
	{
		anyhow::bail!(
			"Key file(s) already exist, please delete them manually from {}",
			output_path.to_string_lossy()
		);
	}

	println!("Generating fresh keys for you Chainflip Node!");

	fs::write(output_path.join("node_key_file"), api::generate_node_key())?;
	fs::write(output_path.join("ethereum_key_file"), api::generate_ethereum_key())?;
	fs::write(output_path.join("signing_key_file"), api::generate_signing_key(None)?)?;

	println!("Saved all secret keys to {}", output_path.to_string_lossy());

	Ok(())
}
