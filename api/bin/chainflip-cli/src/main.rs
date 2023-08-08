#![feature(absolute_path)]
use anyhow::{Context, Result};
use clap::Parser;
use futures::FutureExt;
use serde::Serialize;
use std::{io::Write, path::PathBuf, sync::Arc};

use crate::settings::{
	BrokerSubcommands, CLICommandLineOptions, CLISettings, CliCommand::*,
	LiquidityProviderSubcommands,
};
use api::{
	lp::LpApi, primitives::RedemptionAmount, AccountId32, BrokerApi, GovernanceApi, KeyPair,
	OperatorApi, StateChainApi, SwapDepositAddress,
};
use cf_chains::eth::Address as EthereumAddress;
use chainflip_api as api;
use utilities::{clean_hex_address, task_scope::task_scope};

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

	task_scope(|scope| {
		async move {
			let api = StateChainApi::connect(scope, cli_settings.state_chain).await?;
			match command_line_opts.cmd {
				Broker(BrokerSubcommands::RequestSwapDepositAddress(params)) => {
					let SwapDepositAddress { address, expiry_block, .. } = api
						.broker_api()
						.request_swap_deposit_address(
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
				},
				LiquidityProvider(
					LiquidityProviderSubcommands::RequestLiquidityDepositAddress { asset },
				) => {
					let address = api.lp_api().request_liquidity_deposit_address(asset).await?;
					println!("Deposit Address: {address}");
				},
				LiquidityProvider(
					LiquidityProviderSubcommands::RegisterEmergencyWithdrawalAddress {
						chain,
						address,
					},
				) => {
					let ewa_address = chainflip_api::clean_foreign_chain_address(chain, &address)?;
					let tx_hash =
						api.lp_api().register_emergency_withdrawal_address(ewa_address).await?;
					println!("Emergency Withdrawal Address registered. Tx hash: {tx_hash}");
				},
				Redeem { amount, eth_address } => {
					request_redemption(api.operator_api(), amount, &eth_address).await?;
				},
				RegisterAccountRole { role } => {
					println!(
					"Submitting `register-account-role` with role: {role:?}. This cannot be reversed for your account.",
				);
					if !confirm_submit() {
						return Ok(())
					}
					let tx_hash = api.operator_api().register_account_role(role).await?;
					println!("Account role set at tx {tx_hash:#x}.");
				},
				Rotate {} => {
					let tx_hash = api.operator_api().rotate_session_keys().await?;
					println!("Session key rotated at tx {tx_hash:#x}.");
				},
				StopBidding {} => {
					api.operator_api().stop_bidding().await?;
				},
				StartBidding {} => {
					api.operator_api().start_bidding().await?;
				},
				VanityName { name } => {
					api.operator_api().set_vanity_name(name).await?;
				},
				ForceRotation {} => {
					api.governance_api().force_rotation().await?;
				},
				GenerateKeys { .. } => unreachable!("GenerateKeys is handled above"),
			};
			Ok(())
		}
		.boxed()
	})
	.await
}

async fn request_redemption(
	api: Arc<impl OperatorApi + Sync>,
	amount: Option<f64>,
	eth_address: &str,
) -> Result<()> {
	// Sanitise data
	let eth_address = EthereumAddress::from_slice(
		clean_hex_address::<[u8; 20]>(eth_address)
			.context("Invalid ETH address supplied")?
			.as_slice(),
	);

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

	let tx_hash = api.request_redemption(amount, eth_address).await?;

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
		ethereum_address: EthereumAddress,
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
			writeln!(f, "👤 Ethereum address: 0x{}", self.ethereum_address)?;
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
