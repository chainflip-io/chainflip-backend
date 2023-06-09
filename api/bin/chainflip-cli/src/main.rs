#![feature(absolute_path)]
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serde::Serialize;
use settings::GenerateKeysOutputType;
use std::{fs, path::PathBuf};

use crate::settings::{
	BrokerSubcommands, CLICommandLineOptions, CLISettings, CliCommand::*,
	LiquidityProviderSubcommands,
};
use api::{
	primitives::{AccountRole, Asset, Hash, RedemptionAmount},
	KeyPair,
};
use chainflip_api as api;
use utilities::clean_eth_address;

mod settings;

#[tokio::main]
async fn main() {
	// TODO: call this implicitly from within the API?
	api::use_chainflip_account_id_encoding();

	std::process::exit(match run_cli().await {
		Ok(_) => 0,
		Err(err) => {
			eprintln!("Error: {err:?}");
			1
		},
	})
}

async fn run_cli() -> Result<()> {
	let command_line_opts = CLICommandLineOptions::parse();

	// Generating keys does not require the settings, so run it before them
	if let GenerateKeys { output_type } = command_line_opts.cmd {
		return generate_keys(output_type)
	}

	let cli_settings = CLISettings::new(command_line_opts.clone()).context("Please ensure your config file path is configured correctly and the file is valid. You can also just set all configurations required command line arguments.\n")?;

	println!(
		"Connecting to state chain node at: `{}` and using private key located at: `{}`",
		cli_settings.state_chain.ws_endpoint,
		cli_settings.state_chain.signing_key_file.display()
	);

	match command_line_opts.cmd {
		Broker(BrokerSubcommands::RequestSwapDepositAddress(params)) =>
			request_swap_deposit_address(&cli_settings.state_chain, params).await,
		LiquidityProvider(LiquidityProviderSubcommands::RequestLiquidityDepositAddress {
			asset,
		}) => request_liquidity_deposit_address(&cli_settings.state_chain, asset).await,
		Redeem { amount, eth_address } =>
			request_redemption(amount, &eth_address, &cli_settings).await,
		RegisterAccountRole { role } => register_account_role(role, &cli_settings).await,
		Rotate {} => rotate_keys(&cli_settings.state_chain).await,
		StopBidding {} => api::stop_bidding(&cli_settings.state_chain).await,
		StartBidding {} => api::start_bidding(&cli_settings.state_chain).await,
		Query { block_hash } => request_block(block_hash, &cli_settings.state_chain).await,
		VanityName { name } => api::set_vanity_name(name, &cli_settings.state_chain).await,
		ForceRotation {} => api::force_rotation(&cli_settings.state_chain).await,
		GenerateKeys { .. } => unreachable!("GenerateKeys is handled above"),
	}
}

pub async fn request_swap_deposit_address(
	state_chain_settings: &settings::StateChain,
	params: settings::SwapRequestParams,
) -> Result<()> {
	let api::SwapDepositAddress { address, expiry_block, .. } = api::request_swap_deposit_address(
		state_chain_settings,
		params.source_asset,
		params.destination_asset,
		chainflip_api::clean_foreign_chain_address(
			params.destination_asset.into(),
			&params.destination_address,
		)?,
		params.broker_commission,
		None,
	)
	.await?;
	println!("Deposit Address: {address}");
	println!("Address expires at block {expiry_block}");
	Ok(())
}

pub async fn request_liquidity_deposit_address(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<()> {
	let address = api::lp::request_liquidity_deposit_address(state_chain_settings, asset).await?;
	println!("Deposit Address: {address}");
	Ok(())
}

pub async fn request_block(
	block_hash: Hash,
	state_chain_settings: &settings::StateChain,
) -> Result<()> {
	match api::request_block(block_hash, state_chain_settings).await {
		Ok(block) => {
			println!("{block:#?}");
			Ok(())
		},
		Err(err) => {
			println!("Could not find block with block hash {block_hash:x?}");
			Err(err)
		},
	}
}

async fn register_account_role(role: AccountRole, settings: &settings::CLISettings) -> Result<()> {
	println!(
        "Submitting `register-account-role` with role: {role:?}. This cannot be reversed for your account.",
    );

	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api::register_account_role(role, &settings.state_chain).await?;
	println!("Account role set at tx {tx_hash:#x}.");

	Ok(())
}

pub async fn rotate_keys(state_chain_settings: &settings::StateChain) -> Result<()> {
	let tx_hash = api::rotate_keys(state_chain_settings).await?;
	println!("Session key rotated at tx {tx_hash:#x}.");

	Ok(())
}

async fn request_redemption(
	amount: Option<f64>,
	eth_address: &str,
	settings: &CLISettings,
) -> Result<()> {
	// Sanitise data
	let eth_address: [u8; 20] = clean_eth_address(eth_address)
		.map_err(anyhow::Error::msg).context("Invalid ETH address supplied")
		.and_then(|eth_address|
			if eth_address == [0; 20] {
				Err(anyhow!("Cannot submit redemption to the zero address. If you really want to do this, use 0x000000000000000000000000000000000000dead instead."))
			} else {
				Ok(eth_address)
			}
		)?;

	let amount = match amount {
		Some(amount_float) => {
			let atomic_amount = (amount_float * 10_f64.powi(18)) as u128;

			println!(
				"Submitting redemption with amount `{}` FLIP (`{}` Flipperinos) to ETH address `0x{}`.",
				amount_float,
				atomic_amount,
				hex::encode(eth_address)
			);

			RedemptionAmount::Exact(atomic_amount)
		},
		None => {
			println!(
				"Submitting redemption with MAX amount to ETH address `0x{}`.",
				hex::encode(eth_address)
			);

			RedemptionAmount::Max
		},
	};

	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api::request_redemption(amount, eth_address, &settings.state_chain).await?;

	println!(
		"Your redemption request has transaction hash: `{tx_hash:#x}`. View your redemption's progress on the funding app."
	);

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

/// Generate the 3 keys required for a chainflip node and output them to a path or as JSON.
fn generate_keys(output_type: Option<GenerateKeysOutputType>) -> Result<()> {
	#[derive(Serialize)]
	struct Keys {
		node_key: KeyPair,
		ethereum_key: KeyPair,
		signing_key: KeyPair,
		signing_key_seed: String,
	}

	// Generate new keys
	let keys = api::generate_signing_key(None).map(|(signing_key, signing_key_seed)| Keys {
		node_key: api::generate_node_key(),
		ethereum_key: api::generate_ethereum_key(),
		signing_key,
		signing_key_seed,
	})?;

	// Output the keys depending on the users selected output type
	match output_type.unwrap_or(GenerateKeysOutputType::Files { path: PathBuf::from(".") }) {
		GenerateKeysOutputType::Json => {
			println!(
				"{}",
				serde_json::to_string_pretty(&keys).expect("Should prettify keys to JSON")
			);
		},
		GenerateKeysOutputType::Files { path } => {
			const NODE_KEY_FILE_NAME: &str = "node_key_file";
			const SIGNING_KEY_FILE_NAME: &str = "signing_key_file";
			const ETHEREUM_KEY_FILE_NAME: &str = "ethereum_key_file";

			let absolute_path_string = std::path::absolute(&path)
				.expect("Failed to get absolute path")
				.to_string_lossy()
				.into_owned();

			if path.is_file() {
				anyhow::bail!("Invalid keys path {}", absolute_path_string);
			}
			if !path.exists() {
				std::fs::create_dir_all(path.clone())?
			}

			let node_key_file = path.join(NODE_KEY_FILE_NAME);
			let signing_key_file = path.join(SIGNING_KEY_FILE_NAME);
			let ethereum_key_file = path.join(ETHEREUM_KEY_FILE_NAME);

			if node_key_file.exists() || signing_key_file.exists() || ethereum_key_file.exists() {
				anyhow::bail!(
				"Key file(s) already exist, please move/delete them manually from {absolute_path_string}"
				);
			}

			println!("Generating fresh keys for your Chainflip Node!");

			fs::write(node_key_file, hex::encode(keys.node_key.secret_key))?;
			println!("🔑 Your Node public key is: 0x{}", hex::encode(keys.node_key.public_key));

			fs::write(ethereum_key_file, hex::encode(keys.ethereum_key.secret_key))?;
			println!(
				"🔑 Your Ethereum public key is: 0x{}",
				hex::encode(keys.ethereum_key.public_key)
			);

			fs::write(signing_key_file, hex::encode(keys.signing_key.secret_key))?;
			println!("🔑 Your Validator key is: 0x{}", hex::encode(keys.signing_key.public_key));
			println!("🌱 Your Validator key seed phrase is: {}", keys.signing_key_seed);

			println!("Saved all secret keys to {absolute_path_string}");
		},
	}

	Ok(())
}
