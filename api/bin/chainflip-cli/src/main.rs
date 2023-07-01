#![feature(absolute_path)]
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serde::Serialize;
use std::{io::Write, path::PathBuf};

use crate::settings::{
	BrokerSubcommands, CLICommandLineOptions, CLISettings, CliCommand::*,
	LiquidityProviderSubcommands,
};
use api::{
	primitives::{AccountRole, Asset, Hash, RedemptionAmount},
	AccountId32, KeyPair,
};
use cf_chains::ForeignChain;
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
	if let GenerateKeys { json, path, seed_phrase } = command_line_opts.cmd {
		return generate_keys(json, path, seed_phrase)
	}

	let cli_settings = CLISettings::new(command_line_opts.clone()).context(
		r#"Please ensure your config file path is configured correctly and the file is valid.
			You can also just set all configurations required as command line arguments."#,
	)?;

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
		LiquidityProvider(LiquidityProviderSubcommands::RegisterEmergencyWithdrawalAddress {
			chain,
			address,
		}) => register_emergency_withdrawal_address(&cli_settings.state_chain, chain, address).await,
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

pub async fn register_emergency_withdrawal_address(
	state_chain_settings: &settings::StateChain,
	chain: ForeignChain,
	address: String,
) -> Result<()> {
	let ewa_address = chainflip_api::clean_foreign_chain_address(chain, &address)?;
	let tx_hash =
		api::lp::register_emergency_withdrawal_address(state_chain_settings, ewa_address).await?;
	println!("Emergency Withdrawal Address registered. Tx hash: {tx_hash}");
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
		.context("Invalid ETH address supplied")
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

const DISCLAIMER: &str = r#"
❗️❗️
❗️ THIS SEED PHRASE ALLOWS YOU TO RECOVER YOUR CHAINFLIP ACCOUNT KEYS AND ETHEREUM KEYS.
❗️ HOWEVER, THIS SEED PHRASE SHOULD ONLY BE USED IN CONJUNCTION WITH THIS UTILITY. NOTABLY,
❗️ IT CANNOT BE USED TO IMPORT YOUR ETHEREUM ADDRESS INTO METAMASK OR ANY OTHER WALLET IMPLEMENTATION.
❗️ THIS IS BY DESIGN: THIS ETHEREUM KEY SHOULD BE USED EXCLUSIVELY BY YOUR CHAINFLIP NODE.
❗️❗️
"#;

/// Entry point for the [settings::CliCommand::GenerateKeys] subcommand.
fn generate_keys(json: bool, path: Option<PathBuf>, seed_phrase: Option<String>) -> Result<()> {
	#[derive(Serialize)]
	struct Keys {
		node_key: KeyPair,
		seed_phrase: String,
		ethereum_key: KeyPair,
		#[serde(with = "hex")]
		ethereum_address: [u8; 20],
		signing_key: KeyPair,
		signing_account_id: AccountId32,
	}

	impl std::fmt::Display for Keys {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			writeln!(f, "🔑 Node public key: 0x{}", hex::encode(&self.node_key.public_key))?;
			writeln!(
				f,
				"🔑 Ethereum public key: 0x{}",
				hex::encode(&self.ethereum_key.public_key)
			)?;
			writeln!(f, "👤 Ethereum address: 0x{}", hex::encode(self.ethereum_address))?;
			writeln!(
				f,
				"🔑 Validator public key: 0x{}",
				hex::encode(&self.signing_key.public_key)
			)?;
			writeln!(f, "👤 Validator account id: {}", self.signing_account_id)?;
			writeln!(f)?;
			writeln!(f, "🌱 Seed phrase: {}", self.seed_phrase)?;
			Ok(())
		}
	}

	impl Keys {
		pub fn new(maybe_seed_phrase: Option<String>) -> Result<Self> {
			let (seed_phrase, signing_key, signing_account_id) =
				api::generate_signing_key(maybe_seed_phrase.as_deref())
					.context("Error while generating signing key.")?;
			let (seed_phrase_eth, ethereum_key, ethereum_address) =
				api::generate_ethereum_key(Some(&seed_phrase))
					.context("Error while generating Ethereum key.")?;
			assert_eq!(seed_phrase, seed_phrase_eth);
			Ok(Keys {
				node_key: api::generate_node_key(),
				seed_phrase,
				ethereum_key,
				ethereum_address,
				signing_key,
				signing_account_id,
			})
		}
	}

	let keys = Keys::new(seed_phrase)?;

	if json {
		println!("{}", serde_json::to_string_pretty(&keys)?);
	} else {
		println!();
		println!("Generated fresh Validator keys for your Chainflip Node!");
		println!();
		println!("{}", keys);
		println!("{}", DISCLAIMER);
	}

	if let Some(path) = path {
		if !path.try_exists().context("Could not determine if the directory path exists.")? {
			std::fs::create_dir_all(&path).context("Unable to create keys directory.")?;
		}
		let path = path.canonicalize().context("Unable to resolve path to keys directory.")?;

		for (name, key) in [
			("node_key", hex::encode(keys.node_key.secret_key)),
			("signing_key", hex::encode(keys.signing_key.secret_key)),
			("ethereum_key", hex::encode(keys.ethereum_key.secret_key)),
		] {
			let filename = [name, "_file"].concat();
			write!(
				std::fs::OpenOptions::new()
					.write(true)
					.create_new(true)
					.open(path.join(&filename))
					.context(format!("Could not open file {filename}."))?,
				"{}",
				key
			)
			.context("Error while writing to file.")?;
		}

		println!();
		println!("💾 Saved all secret keys to '{}'.", path.display());
	} else if !json {
		println!();
		println!("💡 You can save the private key files to a directory using the --path argument:");
		println!("💡 `chainflip-cli --seed $MY_SEED_PHRASE --file $PATH_TO_KEYS_DIR`");
	}

	Ok(())
}
