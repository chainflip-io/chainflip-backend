#![feature(absolute_path)]
use crate::settings::{
	BrokerSubcommands, CLICommandLineOptions, CLISettings, CliCommand::*,
	LiquidityProviderSubcommands,
};
use anyhow::{Context, Result};
use api::{
	lp::LpApi,
	primitives::{RedemptionAmount, FLIP_DECIMALS},
	queries::QueryApi,
	AccountId32, BrokerApi, GovernanceApi, KeyPair, OperatorApi, StateChainApi, SwapDepositAddress,
};
use cf_chains::eth::Address as EthereumAddress;
use chainflip_api as api;
use chainflip_api::primitives::state_chain_runtime;
use clap::Parser;
use futures::FutureExt;
use serde::Serialize;
use std::{io::Write, path::PathBuf, sync::Arc};
use utilities::{clean_hex_address, round_f64, task_scope::task_scope};

mod settings;

#[tokio::main]
async fn main() {
	// TODO: call this implicitly from within the API?
	api::use_chainflip_account_id_encoding();

	std::process::exit(match run_cli().await {
		Ok(_) => 0,
		Err(err) => {
			eprintln!("Error: {err:#}");
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
					let SwapDepositAddress { address, .. } = api
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
							params.boost_fee,
						)
						.await?;
					println!("Deposit Address: {address}");
				},
				LiquidityProvider(
					LiquidityProviderSubcommands::RequestLiquidityDepositAddress {
						asset,
						boost_fee,
					},
				) => {
					let address = api
						.lp_api()
						.request_liquidity_deposit_address(asset, api::WaitFor::InBlock, boost_fee)
						.await?
						.unwrap_details();
					println!("Deposit Address: {address}");
				},
				LiquidityProvider(
					LiquidityProviderSubcommands::RegisterLiquidityRefundAddress { chain, address },
				) => {
					let lra_address = chainflip_api::clean_foreign_chain_address(chain, &address)?;
					let tx_hash =
						api.lp_api().register_liquidity_refund_address(lra_address).await?;
					println!("Liquidity Refund address registered. Tx hash: {tx_hash}");
				},
				Redeem { amount, eth_address, executor_address } => {
					request_redemption(api, amount, eth_address, executor_address).await?;
				},
				BindRedeemAddress { eth_address } => {
					bind_redeem_address(api.operator_api(), &eth_address).await?;
				},
				BindExecutorAddress { eth_address } => {
					bind_executor_address(api.operator_api(), &eth_address).await?;
				},
				GetBoundRedeemAddress {} => {
					get_bound_redeem_address(api.query_api()).await?;
				},
				GetBoundExecutorAddress {} => {
					get_bound_executor_address(api.query_api()).await?;
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
				PreUpdateCheck {} => pre_update_check(api.query_api()).await?,
				ForceRotation {} => {
					api.governance_api().force_rotation().await?;
				},
				GenerateKeys { .. } => unreachable!("GenerateKeys is handled above"),
				CountWitnesses { hash } => {
					count_witnesses(api.query_api(), hash).await?;
				},
			};
			Ok(())
		}
		.boxed()
	})
	.await
}

/// Turns the amount of FLIP into a RedemptionAmount in Flipperinos.
fn flip_to_redemption_amount(amount: Option<f64>) -> RedemptionAmount {
	// Using a set number of decimal places of accuracy to avoid floating point rounding errors
	const MAX_DECIMAL_PLACES: u32 = 6;
	match amount {
		Some(amount_float) => {
			let atomic_amount = ((round_f64(amount_float, MAX_DECIMAL_PLACES) *
				10_f64.powi(MAX_DECIMAL_PLACES as i32)) as u128) *
				10_u128.pow(FLIP_DECIMALS - MAX_DECIMAL_PLACES);
			RedemptionAmount::Exact(atomic_amount)
		},
		None => RedemptionAmount::Max,
	}
}

async fn request_redemption(
	api: StateChainApi,
	amount: Option<f64>,
	supplied_redeem_address: String,
	supplied_executor_address: Option<String>,
) -> Result<()> {
	// Check validity of the redeem address
	let redeem_address = EthereumAddress::from(
		clean_hex_address::<[u8; 20]>(&supplied_redeem_address)
			.context("Invalid redeem address")?,
	);

	// Check the validity of the executor address
	let executor_address = if let Some(address) = supplied_executor_address {
		Some(EthereumAddress::from(
			clean_hex_address::<[u8; 20]>(&address).context("Invalid executor address")?,
		))
	} else {
		None
	};

	// Calculate the redemption amount
	let redeem_amount = flip_to_redemption_amount(amount);
	match redeem_amount {
		RedemptionAmount::Exact(atomic_amount) => {
			println!(
				"Submitting redemption with amount `{}` FLIP (`{atomic_amount}` Flipperinos) to ETH address `{redeem_address:?}`.", amount.expect("Exact must be some")
			);
		},
		RedemptionAmount::Max => {
			println!("Submitting redemption with MAX amount to ETH address `{redeem_address:?}`.");
		},
	};

	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api
		.operator_api()
		.request_redemption(redeem_amount, redeem_address, executor_address)
		.await?;

	println!(
		"Your redemption request has State Chain transaction hash: `{tx_hash:#x}`.\n
		View your redemption's progress on the Auctions app."
	);

	Ok(())
}

async fn bind_redeem_address(api: Arc<impl OperatorApi + Sync>, eth_address: &str) -> Result<()> {
	let eth_address = EthereumAddress::from(
		clean_hex_address::<[u8; 20]>(eth_address).context("Invalid ETH address supplied")?,
	);

	println!(
		"Binding your account to a redemption address is irreversible. You will only ever be able to redeem to this address: {eth_address:?}.",
	);
	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api.bind_redeem_address(eth_address).await?;

	println!("Account bound to redeem address {eth_address}, transaction hash: `{tx_hash:#x}`.");

	Ok(())
}

async fn bind_executor_address(api: Arc<impl OperatorApi + Sync>, eth_address: &str) -> Result<()> {
	let eth_address = EthereumAddress::from(
		clean_hex_address::<[u8; 20]>(eth_address).context("Invalid ETH address supplied")?,
	);

	println!(
		"Binding your account to an executor address is irreversible. You will only ever be able to execute registered redemptions with this address: {eth_address:?}.",
	);
	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = api.bind_executor_address(eth_address).await?;

	println!("Account bound to executor address {eth_address}, transaction hash: `{tx_hash:#x}`.");

	Ok(())
}

async fn get_bound_redeem_address(api: QueryApi) -> Result<()> {
	if let Some(bound_address) = api.get_bound_redeem_address(None, None).await? {
		println!("Your account is bound to redeem address: {bound_address:?}");
	} else {
		println!("Your account is not bound to any redeem address.");
	}

	Ok(())
}

async fn get_bound_executor_address(api: QueryApi) -> Result<()> {
	if let Some(bound_address) = api.get_bound_executor_address(None, None).await? {
		println!("Your account is bound to executor address: {bound_address:?}");
	} else {
		println!("Your account is not bound to any executor address.");
	}

	Ok(())
}

async fn pre_update_check(api: QueryApi) -> Result<()> {
	let can_update = api.pre_update_check(None, None).await?;

	println!("Your node is an authority: {}", can_update.is_authority);
	println!("A rotation is occurring: {}", can_update.rotation);
	if let Some(blocks) = can_update.next_block_in {
		println!("Your validator will produce a block in {} blocks", blocks);
	}

	Ok(())
}

async fn count_witnesses(api: QueryApi, hash: state_chain_runtime::Hash) -> Result<()> {
	let result = api.check_witnesses(None, hash).await?;
	match result {
		Some(value) => {
			println!("Number of authorities who failed to witness it: {}", value.failing_count);
			println!("List of witness votes:\n {:?}", value.validators);
		},
		None => {
			println!("The hash you provided lead to no results")
		},
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
		peer_id: String,
		seed_phrase: String,
		ethereum_key: KeyPair,
		#[serde(with = "hex")]
		ethereum_address: EthereumAddress,
		signing_key: KeyPair,
		signing_account_id: AccountId32,
	}

	impl std::fmt::Display for Keys {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			writeln!(f, "🔑 Node Public Key: 0x{}", hex::encode(&self.node_key.public_key))?;
			writeln!(f, "👤 Node Peer ID: {}", self.peer_id)?;
			writeln!(
				f,
				"🔑 Ethereum Public Key: 0x{}",
				hex::encode(&self.ethereum_key.public_key)
			)?;
			writeln!(f, "👤 Ethereum Address: {:?}", self.ethereum_address)?;
			writeln!(
				f,
				"🔑 Validator Public Key: 0x{}",
				hex::encode(&self.signing_key.public_key)
			)?;
			writeln!(f, "👤 Validator Account ID: {}", self.signing_account_id)?;
			writeln!(f)?;
			writeln!(f, "🌱 Seed Phrase: {}", self.seed_phrase)?;
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
			let (node_key, peer_id) =
				api::generate_node_key().context("Error while generating node key.")?;

			Ok(Keys {
				node_key,
				peer_id: peer_id.to_string(),
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
		eprintln!();
		eprintln!("Generated fresh Validator keys for your Chainflip Node!");
		eprintln!();
		eprintln!("{}", keys);
		eprintln!("{}", DISCLAIMER);
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

		eprintln!();
		eprintln!(" 💾 Saved all secret keys to '{}'.", path.display());
	} else {
		eprintln!();
		eprintln!(
			"💡 You can save the private key files to a directory using the --path argument:"
		);
		eprintln!("💡 `chainflip-cli --seed-phrase $MY_SEED_PHRASE --path $PATH_TO_KEYS_DIR`");
	}

	Ok(())
}

#[test]
fn test_flip_to_redemption_amount() {
	assert_eq!(flip_to_redemption_amount(None), RedemptionAmount::Max);
	assert_eq!(flip_to_redemption_amount(Some(0.0)), RedemptionAmount::Exact(0));
	assert_eq!(flip_to_redemption_amount(Some(-1000.0)), RedemptionAmount::Exact(0));
	assert_eq!(
		flip_to_redemption_amount(Some(199995.0)),
		RedemptionAmount::Exact(199995000000000000000000)
	);

	assert_eq!(
		flip_to_redemption_amount(Some(123456789.000001)),
		RedemptionAmount::Exact(123456789000001000000000000)
	);

	assert_eq!(
		flip_to_redemption_amount(Some(69420.123456)),
		RedemptionAmount::Exact(69420123456000000000000)
	);

	// Specifying more than the allowed precision rounds the result to the allowed precision
	assert_eq!(
		flip_to_redemption_amount(Some(6942000.123456789)),
		RedemptionAmount::Exact(6942000123457000000000000)
	);
	assert_eq!(
		flip_to_redemption_amount(Some(4206900.1234564321)),
		RedemptionAmount::Exact(4206900123456000000000000)
	);
}
