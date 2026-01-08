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

use crate::settings::{
	BrokerSubcommands, CLICommandLineOptions, CLISettings, CliCommand::*,
	LiquidityProviderSubcommands, ValidatorSubcommands,
};
use anyhow::{Context, Result};
use api::{
	lp::LpApi, primitives::EpochIndex, queries::QueryApi, AccountId32, GovernanceApi, KeyPair,
	OperatorApi, StateChainApi, ValidatorApi,
};
use bigdecimal::BigDecimal;
use cf_chains::eth::Address as EthereumAddress;
use cf_utilities::{clean_hex_address, round_f64, task_scope::task_scope};
use chainflip_api::{
	self as api,
	lp::LiquidityDepositChannelDetails,
	primitives::{state_chain_runtime, FLIPPERINOS_PER_FLIP},
	rpc_types::{RebalanceOutcome, RedemptionAmount, RedemptionOutcome},
	Asset, BrokerApi,
};
use clap::Parser;
use futures::FutureExt;
use serde::Serialize;
use std::{io::Write, path::PathBuf, str::FromStr, sync::Arc};

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
			You can also just set all configurations required as command line arguments. In this case, don't specify a config-root path."#,
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
				Broker(subcommand) => match subcommand {
					BrokerSubcommands::WithdrawFees(params) => {
						let withdraw_details = api
							.broker_api()
							.withdraw_fees(params.asset, params.destination_address)
							.await?;
						println!("Withdrawal request successfully submitted: {}", withdraw_details);
					},
					BrokerSubcommands::RegisterAccount => {
						api.broker_api().register_account().await?;
					},
					BrokerSubcommands::DeregisterAccount => {
						api.broker_api().deregister_account().await?;
					},
				},
				LiquidityProvider(subcommand) => match subcommand {
					LiquidityProviderSubcommands::RequestLiquidityDepositAddress {
						asset,
						boost_fee,
					} => {
						let LiquidityDepositChannelDetails { deposit_address, deposit_chain_expiry_block } = api
							.lp_api()
							.request_liquidity_deposit_address_v2(
								asset,
								boost_fee,
							)
							.await?
							.response;
						println!("Deposit Address: {deposit_address}\nDeposit chain expiry block: {deposit_chain_expiry_block}");
					},

					LiquidityProviderSubcommands::RegisterLiquidityRefundAddress {
						chain,
						address,
					} => {
						let tx_hash =
							api.lp_api().register_liquidity_refund_address(chain, address).await?;
						println!("Liquidity Refund address registered. Tx hash: {tx_hash}");
					},
					LiquidityProviderSubcommands::RegisterAccount => {
						api.lp_api().register_account().await?;
					},
					LiquidityProviderSubcommands::DeregisterAccount => {
						api.lp_api().deregister_account().await?;
					},
				},
				Validator(subcommand) => match subcommand {
					ValidatorSubcommands::RegisterAccount => {
						api.validator_api().register_account().await?;
					},
					ValidatorSubcommands::DeregisterAccount => {
						api.validator_api().deregister_account().await?;
					},
					ValidatorSubcommands::StopBidding => {
						let tx_hash = api.validator_api().stop_bidding().await?;
						println!("Account stopped bidding, in tx {tx_hash:#x}.");
					},
					ValidatorSubcommands::StartBidding => {
						let tx_hash = api.validator_api().start_bidding().await?;
						println!("Account started bidding at tx {tx_hash:#x}.");
					},
					ValidatorSubcommands::AcceptOperator { operator_id } => {
						let operator_account = AccountId32::from_str(&operator_id)
							.map_err(|err| anyhow::anyhow!("Failed to parse AccountId: {}", err))
							.context("Invalid account ID provided")?;
						let events = api.validator_api().accept_operator(operator_account).await?;
						println!("Operator accepted. Events: {:#?}", events);
					},
					ValidatorSubcommands::RemoveOperator => {
						let events = api.validator_api().remove_operator().await?;
						println!("Operator removed. Events: {:#?}", events);
					},
				},
				Redeem { amount, eth_address, executor_address } => {
					request_rebalance_or_redemption(
						api,
						amount,
						RedeemDestination::parse_external(eth_address, executor_address)?
					).await?;
				},
				Rebalance { amount, recipient_account_id, restricted_address } => {
					request_rebalance_or_redemption(
						api,
						amount,
						RedeemDestination::parse_internal(recipient_account_id, restricted_address)?
					).await?;
				},
				BindRedeemAddress { eth_address } => {
					bind_redeem_address(api.operator_api(), &eth_address).await?;
				},
				BindExecutorAddress { eth_address } => {
					bind_executor_address(api.operator_api(), &eth_address).await?;
				},
				GetBoundRedeemAddress => {
					get_bound_redeem_address(api.query_api()).await?;
				},
				GetBoundExecutorAddress => {
					get_bound_executor_address(api.query_api()).await?;
				},
				Rotate => {
					let tx_hash = api.operator_api().rotate_session_keys().await?;
					println!("Session key rotated at tx {tx_hash:#x}.");
				},
				StopBidding => {
					let tx_hash = api.validator_api().stop_bidding().await?;
					println!("Account stopped bidding, in tx {tx_hash:#x}.");
				},
				StartBidding => {
					let tx_hash = api.validator_api().start_bidding().await?;
					println!("Account started bidding at tx {tx_hash:#x}.");
				},
				VanityName { name } => {
					api.operator_api().set_vanity_name(name).await?;
				},
				PreUpdateCheck => pre_update_check(api.query_api()).await?,
				ForceRotation => {
					api.governance_api().force_rotation().await?;
				},
				GenerateKeys { .. } => unreachable!("GenerateKeys is handled above"),
				CountWitnesses { hash, epoch_index } => {
					count_witnesses(api.query_api(), hash, epoch_index).await?;
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
				10_u128.pow(Asset::Flip.decimals() - MAX_DECIMAL_PLACES);
			RedemptionAmount::Exact(atomic_amount)
		},
		None => RedemptionAmount::Max,
	}
}

/// Turns an amount in Flipperinos back into a string representing
/// this amount in FLIP.
fn flipperino_to_flip_string(atomic_amount: u128) -> String {
	(BigDecimal::from(atomic_amount) / FLIPPERINOS_PER_FLIP).to_string()
}

enum RedeemDestination {
	External { redeem_address: EthereumAddress, executor_address: Option<EthereumAddress> },
	Internal { recipient_account_id: AccountId32, restricted_address: Option<EthereumAddress> },
}

impl RedeemDestination {
	fn parse_external(redeem_address: String, executor_address: Option<String>) -> Result<Self> {
		Ok(Self::External {
			redeem_address: EthereumAddress::from(
				clean_hex_address::<[u8; 20]>(&redeem_address)
					.context("Invalid ETH redeem address")?,
			),
			executor_address: executor_address
				.map(|address| {
					clean_hex_address::<[u8; 20]>(&address)
						.context("Invalid executor address")
						.map(EthereumAddress::from)
				})
				.transpose()?,
		})
	}
	fn parse_internal(
		recipient_account_id: String,
		restricted_address: Option<String>,
	) -> Result<Self> {
		use std::str::FromStr;
		Ok(Self::Internal {
			recipient_account_id: AccountId32::from_str(&recipient_account_id)
				.map_err(|err| anyhow::anyhow!("Failed to parse AccountId: {}", err))
				.context("Invalid account ID provided")?,
			restricted_address: restricted_address
				.map(|address| {
					clean_hex_address::<[u8; 20]>(&address)
						.context("Invalid address")
						.map(EthereumAddress::from)
				})
				.transpose()?,
		})
	}
}

async fn request_rebalance_or_redemption(
	api: StateChainApi,
	amount: Option<f64>,
	destination: RedeemDestination,
) -> Result<()> {
	let redeem_amount = flip_to_redemption_amount(amount);

	println!(
		"Submitting request to {} {} to {}...",
		match destination {
			RedeemDestination::External { .. } => "redeem",
			RedeemDestination::Internal { .. } => "rebalance",
		},
		match redeem_amount {
			RedemptionAmount::Exact(atomic_amount) => format!(
				"`{}` FLIP (`{}` Flipperinos)",
				flipperino_to_flip_string(atomic_amount),
				atomic_amount,
			),
			RedemptionAmount::Max => "MAX amount of FLIP".to_string(),
		},
		match destination {
			RedeemDestination::External { ref redeem_address, .. } =>
				format!("ETH address `{redeem_address:?}`"),
			RedeemDestination::Internal { ref recipient_account_id, .. } =>
				format!("account `{recipient_account_id}`"),
		},
	);

	if !confirm_submit() {
		return Ok(())
	}

	match destination {
		RedeemDestination::External { redeem_address, executor_address } => {
			let RedemptionOutcome { source_account_id, redeem_address, amount, .. } = api
				.operator_api()
				.request_redemption(redeem_amount, redeem_address, executor_address)
				.await?;

			println!(
				"Redemption request succeeded: a redemption for {} FLIP from account {} to address {:?} will be initiated on Ethereum.\nView your redemption's progress on the Auctions app.",
				flipperino_to_flip_string(amount),
				source_account_id,
				redeem_address,
			);
		},
		RedeemDestination::Internal { recipient_account_id, restricted_address } => {
			println!("Waiting for finality...");
			let RebalanceOutcome { source_account_id, recipient_account_id, amount } = api
				.operator_api()
				.request_rebalance(redeem_amount, restricted_address, recipient_account_id)
				.await?;

			println!(
				"Rebalance request succeeded: {} FLIP transferred from account {} to {}.",
				flipperino_to_flip_string(amount),
				source_account_id,
				recipient_account_id,
			);
		},
	}

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

async fn count_witnesses(
	api: QueryApi,
	hash: state_chain_runtime::Hash,
	epoch_index: Option<EpochIndex>,
) -> Result<()> {
	let result = api.check_witnesses(None, hash, epoch_index).await?;
	match result {
		Some(value) => {
			println!("Number of authorities who failed to witness it: {}", value.failing_count);
			println!("List of witness votes:\n {:?}", value.validators);
		},
		None => {
			println!("The hash you provided leads to no results")
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
		eprintln!("💡 `chainflip-cli generate-keys --seed-phrase $MY_SEED_PHRASE --path $PATH_TO_KEYS_DIR`");
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

#[test]
fn test_flipperino_flip_roundtrip() {
	fn assert_eq_flip_string(amount: f64, result: String) {
		match flip_to_redemption_amount(Some(amount)) {
			RedemptionAmount::Max => panic!("Expected exact amount."),
			RedemptionAmount::Exact(amount) =>
				assert_eq!(flipperino_to_flip_string(amount), result),
		}
	}
	assert_eq_flip_string(0.0, "0".into());
	assert_eq_flip_string(1.0, "1".into());
	assert_eq_flip_string(0.1, "0.1".into());
	assert_eq_flip_string(17777.777777777777, "17777.777778".into());
	assert_eq_flip_string(0.0000009, "0.000001".into());
	assert_eq_flip_string(0.50000001, "0.5".into());
}
