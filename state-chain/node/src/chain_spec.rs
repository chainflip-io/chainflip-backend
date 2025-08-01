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

use cf_chains::{
	arb::ArbitrumTrackedData,
	assets::btc,
	btc::{BitcoinFeeInfo, BitcoinTrackedData, BITCOIN_DUST_LIMIT},
	dot::{PolkadotAccountId, PolkadotHash, PolkadotTrackedData, RuntimeVersion},
	eth::EthereumTrackedData,
	hub::AssethubTrackedData,
	sol::{
		api::DurableNonceAndAccount, AddressLookupTableAccount, SolAddress, SolApiEnvironment,
		SolHash, SolTrackedData,
	},
	Arbitrum, Assethub, Bitcoin, ChainState, Ethereum, Polkadot,
};
use cf_primitives::{
	chains::Solana, AccountRole, AuthorityCount, NetworkEnvironment,
	DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
};
use common::{FLIPPERINOS_PER_FLIP, SHARED_DATA_REFERENCE_LIFETIME};
use pallet_cf_elections::generic_tools::{ArrayContainer, ArrayToVector, CommonTraits};
pub use sc_service::{ChainType, Properties};
use sc_telemetry::serde_json::json;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{
	crypto::{set_default_ss58_version, Ss58AddressFormat, UncheckedInto},
	Pair, Public,
};
use state_chain_runtime::{
	chainflip::{
		bitcoin_elections,
		generic_elections::{self, ChainlinkOraclePriceSettings},
		solana_elections, Offence,
	},
	constants::common::{
		BLOCKS_PER_MINUTE_ARBITRUM, BLOCKS_PER_MINUTE_ASSETHUB, BLOCKS_PER_MINUTE_ETHEREUM,
		BLOCKS_PER_MINUTE_POLKADOT, BLOCKS_PER_MINUTE_SOLANA,
	},
	opaque::SessionKeys,
	AccountId, BitcoinElectionsConfig, BlockNumber, FlipBalance, GenericElectionsConfig,
	SetSizeParameters, Signature, SolanaElectionsConfig, WASM_BINARY,
};

use cf_utilities::clean_hex_address;
use std::{
	collections::{BTreeMap, BTreeSet},
	env,
	str::FromStr,
	time::{SystemTime, UNIX_EPOCH},
};

use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	BoundedVec, Percent, Permill,
};

pub mod berghain;
pub mod common;
pub mod devnet;
pub mod perseverance;
pub mod sisyphos;
pub mod testnet;

/// Generate a crypto pair from seed.
pub fn test_account_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{seed}"), None)
		.expect("static values are valid; qed")
		.public()
}

pub fn parse_account(ss58: &str) -> AccountId {
	AccountId::from_str(ss58).unwrap_or_else(|_| panic!("Invalid address: {}", ss58))
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(test_account_from_seed::<TPublic>(seed)).into_account()
}

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

/// generate session keys from Aura and Grandpa keys
pub fn session_keys(aura: AuraId, grandpa: GrandpaId) -> SessionKeys {
	SessionKeys { aura, grandpa }
}
pub struct StateChainEnvironment {
	flip_token_address: [u8; 20],
	eth_usdc_address: [u8; 20],
	eth_usdt_address: [u8; 20],
	state_chain_gateway_address: [u8; 20],
	eth_key_manager_address: [u8; 20],
	eth_vault_address: [u8; 20],
	eth_address_checker_address: [u8; 20],
	ethereum_chain_id: u64,
	eth_init_agg_key: [u8; 33],
	sol_init_agg_key: Option<SolAddress>,
	arb_key_manager_address: [u8; 20],
	arb_vault_address: [u8; 20],
	arb_usdc_token_address: [u8; 20],
	arb_address_checker_address: [u8; 20],
	arbitrum_chain_id: u64,
	ethereum_deployment_block: u64,
	genesis_funding_amount: u128,
	/// Note: Minimum funding should be expressed in Flipperinos.
	min_funding: u128,
	dot_genesis_hash: PolkadotHash,
	dot_vault_account_id: Option<PolkadotAccountId>,
	dot_runtime_version: RuntimeVersion,
	hub_genesis_hash: PolkadotHash,
	hub_vault_account_id: Option<PolkadotAccountId>,
	hub_runtime_version: RuntimeVersion,
	// Solana related
	sol_genesis_hash: Option<SolHash>,
	sol_vault_program: SolAddress,
	sol_vault_program_data_account: SolAddress,
	sol_usdc_token_mint_pubkey: SolAddress,
	sol_token_vault_pda_account: SolAddress,
	sol_usdc_token_vault_ata: SolAddress,
	// We injected 10 nonce accounts at genesis and 40 more on an upgrade
	sol_durable_nonces_and_accounts: [DurableNonceAndAccount; 50],
	sol_swap_endpoint_program: SolAddress,
	sol_swap_endpoint_program_data_account: SolAddress,
	sol_alt_manager_program: SolAddress,
	// Initialized with 65 accounts (50 of them nonces)
	sol_address_lookup_table_account: (SolAddress, [SolAddress; 65]),
	chainlink_oracle_price_settings: ChainlinkOraclePriceSettings<ArrayContainer<5>>,
}

/// Get the values from the State Chain's environment variables. Else set them via the defaults
pub fn get_environment_or_defaults(defaults: StateChainEnvironment) -> StateChainEnvironment {
	macro_rules! from_env_var {
		( $parse:path, $env_var:ident, $name:ident ) => {
			let $name = match env::var(stringify!($env_var)) {
				Ok(s) => $parse(&s)
					.expect(format!("Unable to parse env var {}.", stringify!($env_var)).as_str()),
				Err(_) => defaults.$name,
			};
		};
	}
	fn hex_decode<const S: usize>(s: &str) -> Result<[u8; S], String> {
		hex::decode(s)
			.map_err(|e| e.to_string())?
			.try_into()
			.map_err(|_| "Incorrect length of hex string.".into())
	}
	from_env_var!(clean_hex_address, FLIP_TOKEN_ADDRESS, flip_token_address);
	from_env_var!(clean_hex_address, ETH_USDC_ADDRESS, eth_usdc_address);
	from_env_var!(clean_hex_address, ETH_USDT_ADDRESS, eth_usdt_address);
	from_env_var!(clean_hex_address, STATE_CHAIN_GATEWAY_ADDRESS, state_chain_gateway_address);
	from_env_var!(clean_hex_address, KEY_MANAGER_ADDRESS, eth_key_manager_address);
	from_env_var!(clean_hex_address, ETH_VAULT_ADDRESS, eth_vault_address);
	from_env_var!(clean_hex_address, ARB_KEY_MANAGER_ADDRESS, arb_key_manager_address);
	from_env_var!(clean_hex_address, ARB_VAULT_ADDRESS, arb_vault_address);
	from_env_var!(clean_hex_address, ARB_USDC_TOKEN_ADDRESS, arb_usdc_token_address);
	from_env_var!(clean_hex_address, ADDRESS_CHECKER_ADDRESS, eth_address_checker_address);
	from_env_var!(clean_hex_address, ARB_ADDRESS_CHECKER, arb_address_checker_address);
	from_env_var!(hex_decode, ETH_INIT_AGG_KEY, eth_init_agg_key);
	from_env_var!(FromStr::from_str, ETHEREUM_CHAIN_ID, ethereum_chain_id);
	from_env_var!(FromStr::from_str, ARBITRUM_CHAIN_ID, arbitrum_chain_id);
	from_env_var!(FromStr::from_str, ETH_DEPLOYMENT_BLOCK, ethereum_deployment_block);
	from_env_var!(FromStr::from_str, GENESIS_FUNDING, genesis_funding_amount);
	from_env_var!(FromStr::from_str, MIN_FUNDING, min_funding);
	from_env_var!(FromStr::from_str, SOL_VAULT_ADDRESS, sol_vault_program);
	from_env_var!(
		FromStr::from_str,
		SOL_VAULT_PROGRAM_DATA_ACCOUNT,
		sol_vault_program_data_account
	);
	from_env_var!(FromStr::from_str, SOL_TOKEN_VAULT_PDA_ACCOUNT, sol_token_vault_pda_account);
	from_env_var!(FromStr::from_str, SOL_USDC_TOKEN_MINT_PUBKEY, sol_usdc_token_mint_pubkey);
	from_env_var!(FromStr::from_str, SOL_USDC_TOKEN_VAULT_ATA, sol_usdc_token_vault_ata);
	from_env_var!(FromStr::from_str, SOL_SWAP_ENDPOINT_PROGRAM, sol_swap_endpoint_program);
	from_env_var!(
		FromStr::from_str,
		SOL_SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT,
		sol_swap_endpoint_program_data_account
	);
	from_env_var!(FromStr::from_str, SOL_ALT_MANAGER_PROGRAM, sol_alt_manager_program);

	let dot_genesis_hash = match env::var("DOT_GENESIS_HASH") {
		Ok(s) => hex_decode::<32>(&s).unwrap().into(),
		Err(_) => defaults.dot_genesis_hash,
	};
	let dot_vault_account_id = match env::var("DOT_VAULT_ACCOUNT_ID") {
		Ok(s) => Some(PolkadotAccountId::from_aliased(hex_decode::<32>(&s).unwrap())),
		Err(_) => defaults.dot_vault_account_id,
	};

	let dot_spec_version: u32 = match env::var("DOT_SPEC_VERSION") {
		Ok(s) => s.parse().unwrap(),
		Err(_) => defaults.dot_runtime_version.spec_version,
	};
	let dot_transaction_version: u32 = match env::var("DOT_TRANSACTION_VERSION") {
		Ok(s) => s.parse().unwrap(),
		Err(_) => defaults.dot_runtime_version.transaction_version,
	};

	let hub_genesis_hash = match env::var("HUB_GENESIS_HASH") {
		Ok(s) => hex_decode::<32>(&s).unwrap().into(),
		Err(_) => defaults.hub_genesis_hash,
	};
	let hub_vault_account_id = match env::var("HUB_VAULT_ACCOUNT_ID") {
		Ok(s) => Some(PolkadotAccountId::from_aliased(hex_decode::<32>(&s).unwrap())),
		Err(_) => defaults.hub_vault_account_id,
	};
	let hub_spec_version: u32 = match env::var("HUB_SPEC_VERSION") {
		Ok(s) => s.parse().unwrap(),
		Err(_) => defaults.hub_runtime_version.spec_version,
	};
	let hub_transaction_version: u32 = match env::var("HUB_TRANSACTION_VERSION") {
		Ok(s) => s.parse().unwrap(),
		Err(_) => defaults.hub_runtime_version.transaction_version,
	};

	let sol_genesis_hash = match env::var("SOL_GENESIS_HASH") {
		Ok(s) => Some(SolHash::from_str(&s).unwrap()),
		Err(_) => defaults.sol_genesis_hash,
	};

	let sol_durable_nonces_and_accounts = match env::var("SOL_NONCES_AND_ACCOUNTS") {
		Ok(_) => unimplemented!("Solana nonces and nonce accounts should not be supplied via environment variables since its a complex type"),
		Err(_) => defaults.sol_durable_nonces_and_accounts,
	};

	let sol_address_lookup_table_account = match env::var("SOL_ADDRESS_LOOKUP_TABLE_ACCOUNT") {
		Ok(_) => unimplemented!("Solana address lookup table account should not be supplied via environment variables since its a complex type"),
		Err(_) => defaults.sol_address_lookup_table_account,
	};

	let sol_init_agg_key = match env::var("SOL_INIT_AGG_KEY") {
		Ok(_) => unimplemented!(
			"Solana initial agg key should not be supplied via environment variables"
		),
		Err(_) => defaults.sol_init_agg_key,
	};

	let chainlink_oracle_price_settings = match env::var("CHAINLINK_ORACLE_PRICE_SETTINGS") {
		Ok(_) => unimplemented!(
			"Oracle price election settings should not be supplied via environment variables"
		),
		Err(_) => defaults.chainlink_oracle_price_settings,
	};

	StateChainEnvironment {
		flip_token_address,
		eth_usdc_address,
		eth_usdt_address,
		state_chain_gateway_address,
		eth_key_manager_address,
		eth_vault_address,
		arb_key_manager_address,
		arb_vault_address,
		arb_usdc_token_address,
		eth_address_checker_address,
		arb_address_checker_address,
		ethereum_chain_id,
		arbitrum_chain_id,
		eth_init_agg_key,
		sol_init_agg_key,
		ethereum_deployment_block,
		genesis_funding_amount,
		min_funding,
		dot_genesis_hash,
		dot_vault_account_id,
		dot_runtime_version: RuntimeVersion {
			spec_version: dot_spec_version,
			transaction_version: dot_transaction_version,
		},
		hub_genesis_hash,
		hub_vault_account_id,
		hub_runtime_version: RuntimeVersion {
			spec_version: hub_spec_version,
			transaction_version: hub_transaction_version,
		},
		sol_genesis_hash,
		sol_vault_program,
		sol_vault_program_data_account,
		sol_usdc_token_mint_pubkey,
		sol_token_vault_pda_account,
		sol_usdc_token_vault_ata,
		sol_durable_nonces_and_accounts,
		sol_swap_endpoint_program,
		sol_swap_endpoint_program_data_account,
		sol_alt_manager_program,
		sol_address_lookup_table_account,
		chainlink_oracle_price_settings,
	}
}

/// Start a single node development chain - using bashful as genesis node
pub fn cf_development_config() -> Result<ChainSpec, String> {
	inner_cf_development_config(vec![(
		parse_account(testnet::BASHFUL_ACCOUNT_ID),
		testnet::BASHFUL_SR25519.unchecked_into(),
		testnet::BASHFUL_ED25519.unchecked_into(),
	)])
}

/// Start a three node development chain - using bashful, doc and dopey as genesis nodes
pub fn cf_three_node_development_config() -> Result<ChainSpec, String> {
	inner_cf_development_config(vec![
		(
			parse_account(testnet::BASHFUL_ACCOUNT_ID),
			testnet::BASHFUL_SR25519.unchecked_into(),
			testnet::BASHFUL_ED25519.unchecked_into(),
		),
		(
			parse_account(testnet::DOC_ACCOUNT_ID),
			testnet::DOC_SR25519.unchecked_into(),
			testnet::DOC_ED25519.unchecked_into(),
		),
		(
			parse_account(testnet::DOPEY_ACCOUNT_ID),
			testnet::DOPEY_SR25519.unchecked_into(),
			testnet::DOPEY_ED25519.unchecked_into(),
		),
	])
}

pub fn inner_cf_development_config(
	initial_authorities: Vec<(AccountId, AuraId, GrandpaId)>,
) -> Result<ChainSpec, String> {
	let wasm_binary =
		WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;

	let StateChainEnvironment {
		flip_token_address,
		eth_usdc_address,
		eth_usdt_address,
		state_chain_gateway_address,
		eth_key_manager_address,
		eth_vault_address,
		arb_key_manager_address,
		arb_vault_address,
		arb_usdc_token_address,
		eth_address_checker_address,
		arb_address_checker_address,
		ethereum_chain_id,
		arbitrum_chain_id,
		eth_init_agg_key,
		sol_init_agg_key,
		ethereum_deployment_block,
		genesis_funding_amount,
		min_funding,
		dot_genesis_hash,
		dot_vault_account_id,
		dot_runtime_version,
		hub_genesis_hash,
		hub_vault_account_id,
		hub_runtime_version,
		sol_genesis_hash,
		sol_vault_program,
		sol_vault_program_data_account,
		sol_usdc_token_mint_pubkey,
		sol_token_vault_pda_account,
		sol_usdc_token_vault_ata,
		sol_durable_nonces_and_accounts,
		sol_swap_endpoint_program,
		sol_swap_endpoint_program_data_account,
		sol_alt_manager_program,
		sol_address_lookup_table_account,
		chainlink_oracle_price_settings,
	} = get_environment_or_defaults(testnet::ENV);
	Ok(ChainSpec::builder(wasm_binary, Default::default())
		.with_name("CF Develop")
		.with_id("cf-dev")
		.with_protocol_id("flip-dev")
		.with_chain_type(ChainType::Development)
		.with_genesis_config(testnet_genesis(
			initial_authorities.clone(),
			testnet::extra_accounts(),
			// Governance account - Snow White
			testnet::SNOW_WHITE_SR25519.into(),
			devnet::MIN_AUTHORITIES,
			devnet::AUCTION_PARAMETERS,
			DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
			state_chain_runtime::EnvironmentConfig {
				flip_token_address: flip_token_address.into(),
				eth_usdc_address: eth_usdc_address.into(),
				eth_usdt_address: eth_usdt_address.into(),
				state_chain_gateway_address: state_chain_gateway_address.into(),
				eth_key_manager_address: eth_key_manager_address.into(),
				eth_vault_address: eth_vault_address.into(),
				eth_address_checker_address: eth_address_checker_address.into(),
				arb_key_manager_address: arb_key_manager_address.into(),
				arb_vault_address: arb_vault_address.into(),
				arb_address_checker_address: arb_address_checker_address.into(),
				arb_usdc_address: arb_usdc_token_address.into(),
				ethereum_chain_id,
				arbitrum_chain_id,
				polkadot_genesis_hash: dot_genesis_hash,
				polkadot_vault_account_id: dot_vault_account_id,
				assethub_genesis_hash: hub_genesis_hash,
				assethub_vault_account_id: hub_vault_account_id,
				sol_genesis_hash,
				sol_api_env: SolApiEnvironment {
					vault_program: sol_vault_program,
					vault_program_data_account: sol_vault_program_data_account,
					usdc_token_mint_pubkey: sol_usdc_token_mint_pubkey,
					token_vault_pda_account: sol_token_vault_pda_account,
					usdc_token_vault_ata: sol_usdc_token_vault_ata,
					swap_endpoint_program: sol_swap_endpoint_program,
					swap_endpoint_program_data_account: sol_swap_endpoint_program_data_account,
					alt_manager_program: sol_alt_manager_program,
					address_lookup_table_account: AddressLookupTableAccount {
						key: sol_address_lookup_table_account.0.into(),
						addresses: sol_address_lookup_table_account
							.1
							.into_iter()
							.map(|addr| addr.into())
							.collect(),
					},
				},
				sol_durable_nonces_and_accounts: sol_durable_nonces_and_accounts.to_vec(),
				network_environment: NetworkEnvironment::Development,
				..Default::default()
			},
			eth_init_agg_key,
			sol_init_agg_key,
			ethereum_deployment_block,
			devnet::TOTAL_ISSUANCE,
			common::DAILY_SLASHING_RATE,
			genesis_funding_amount,
			min_funding,
			devnet::REDEMPTION_TAX,
			8 * devnet::HOURS,
			devnet::REDEMPTION_TTL_SECS,
			devnet::CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
			devnet::BACKUP_NODE_EMISSION_INFLATION_PERBILL,
			devnet::EXPIRY_SPAN_IN_SECONDS,
			devnet::ACCRUAL_RATIO,
			Percent::from_percent(devnet::REDEMPTION_PERIOD_AS_PERCENTAGE),
			devnet::SUPPLY_UPDATE_INTERVAL,
			devnet::PENALTIES.to_vec(),
			devnet::KEYGEN_CEREMONY_TIMEOUT_BLOCKS,
			devnet::THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS,
			dot_runtime_version,
			hub_runtime_version,
			// Bitcoin block times on localnets are much faster, so we account for that here.
			devnet::BITCOIN_EXPIRY_BLOCKS,
			devnet::ETHEREUM_EXPIRY_BLOCKS,
			devnet::ARBITRUM_EXPIRY_BLOCKS,
			devnet::POLKADOT_EXPIRY_BLOCKS,
			devnet::SOLANA_EXPIRY_BLOCKS,
			devnet::ASSETHUB_EXPIRY_BLOCKS,
			devnet::BITCOIN_SAFETY_MARGIN,
			devnet::ETHEREUM_SAFETY_MARGIN,
			devnet::ARBITRUM_SAFETY_MARGIN,
			devnet::SOLANA_SAFETY_MARGIN,
			devnet::AUCTION_BID_CUTOFF_PERCENTAGE,
			SolanaElectionsConfig {
				option_initial_state: Some(solana_elections::initial_state(
					sol_vault_program,
					sol_usdc_token_mint_pubkey,
					sol_swap_endpoint_program_data_account,
					SHARED_DATA_REFERENCE_LIFETIME,
				)),
			},
			BitcoinElectionsConfig {
				option_initial_state: Some(bitcoin_elections::initial_state()),
			},
			GenericElectionsConfig {
				option_initial_state: Some(generic_elections::initial_state(
					chainlink_oracle_price_settings.convert(ArrayToVector),
				)),
			},
		))
		.build())
}

macro_rules! network_spec {
	( $network:ident ) => {
		impl $network::Config {
			pub fn build_spec(
				env_override: Option<StateChainEnvironment>,
			) -> Result<ChainSpec, String> {
				use $network::*;
				assert_eq!(
					parse_account(SNOW_WHITE_ACCOUNT_ID).as_ref(),
					SNOW_WHITE_SR25519,
					"Snow White account ID does not match the public key."
				);

				let wasm_binary =
					WASM_BINARY.ok_or_else(|| "Wasm binary not available".to_string())?;
				let StateChainEnvironment {
					flip_token_address,
					eth_usdc_address,
					eth_usdt_address,
					state_chain_gateway_address,
					eth_key_manager_address,
					eth_vault_address,
					arb_key_manager_address,
					arb_vault_address,
					arb_usdc_token_address,
					eth_address_checker_address,
					arb_address_checker_address,
					ethereum_chain_id,
					arbitrum_chain_id,
					eth_init_agg_key,
					sol_init_agg_key,
					ethereum_deployment_block,
					genesis_funding_amount,
					min_funding,
					dot_genesis_hash,
					dot_vault_account_id,
					dot_runtime_version,
					hub_genesis_hash,
					hub_vault_account_id,
					hub_runtime_version,
					sol_genesis_hash,
					sol_vault_program,
					sol_vault_program_data_account,
					sol_usdc_token_mint_pubkey,
					sol_token_vault_pda_account,
					sol_usdc_token_vault_ata,
					sol_durable_nonces_and_accounts,
					sol_swap_endpoint_program,
					sol_swap_endpoint_program_data_account,
					sol_alt_manager_program,
					sol_address_lookup_table_account,
					chainlink_oracle_price_settings,
				} = env_override.unwrap_or(ENV);
				let protocol_id = format!(
					"{}-{}",
					PROTOCOL_ID,
					hex::encode(
						&SystemTime::now()
							.duration_since(UNIX_EPOCH)
							.unwrap()
							.as_secs()
							.to_be_bytes()[4..],
					)
				);
				Ok(ChainSpec::builder(wasm_binary, Default::default())
					.with_name(NETWORK_NAME)
					.with_id(NETWORK_NAME)
					.with_protocol_id(&protocol_id)
					.with_chain_type(CHAIN_TYPE)
					.with_properties(chainflip_properties())
					.with_genesis_config(testnet_genesis(
						// Initial PoA authorities
						vec![
							(
								parse_account(BASHFUL_ACCOUNT_ID),
								BASHFUL_SR25519.unchecked_into(),
								BASHFUL_ED25519.unchecked_into(),
							),
							(
								parse_account(DOC_ACCOUNT_ID),
								DOC_SR25519.unchecked_into(),
								DOC_ED25519.unchecked_into(),
							),
							(
								parse_account(DOPEY_ACCOUNT_ID),
								DOPEY_SR25519.unchecked_into(),
								DOPEY_ED25519.unchecked_into(),
							),
						],
						// Extra accounts
						$network::extra_accounts(),
						// Governance account - Snow White
						SNOW_WHITE_SR25519.into(),
						MIN_AUTHORITIES,
						AUCTION_PARAMETERS,
						DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
						state_chain_runtime::EnvironmentConfig {
							flip_token_address: flip_token_address.into(),
							eth_usdc_address: eth_usdc_address.into(),
							eth_usdt_address: eth_usdt_address.into(),
							state_chain_gateway_address: state_chain_gateway_address.into(),
							eth_key_manager_address: eth_key_manager_address.into(),
							eth_vault_address: eth_vault_address.into(),
							eth_address_checker_address: eth_address_checker_address.into(),
							arb_key_manager_address: arb_key_manager_address.into(),
							arb_vault_address: arb_vault_address.into(),
							arb_address_checker_address: arb_address_checker_address.into(),
							arb_usdc_address: arb_usdc_token_address.into(),
							ethereum_chain_id,
							arbitrum_chain_id,
							polkadot_genesis_hash: dot_genesis_hash,
							polkadot_vault_account_id: dot_vault_account_id.clone(),
							assethub_genesis_hash: hub_genesis_hash,
							assethub_vault_account_id: hub_vault_account_id.clone(),
							sol_genesis_hash,
							sol_durable_nonces_and_accounts: sol_durable_nonces_and_accounts
								.to_vec(),
							sol_api_env: SolApiEnvironment {
								vault_program: sol_vault_program,
								vault_program_data_account: sol_vault_program_data_account,
								usdc_token_mint_pubkey: sol_usdc_token_mint_pubkey,
								token_vault_pda_account: sol_token_vault_pda_account,
								usdc_token_vault_ata: sol_usdc_token_vault_ata,
								swap_endpoint_program: sol_swap_endpoint_program,
								swap_endpoint_program_data_account:
									sol_swap_endpoint_program_data_account,
								alt_manager_program: sol_alt_manager_program,
								address_lookup_table_account: AddressLookupTableAccount {
									key: sol_address_lookup_table_account.0.into(),
									addresses: sol_address_lookup_table_account
										.1
										.into_iter()
										.map(|addr| addr.into())
										.collect(),
								},
							},
							network_environment: NETWORK_ENVIRONMENT,
							..Default::default()
						},
						eth_init_agg_key,
						sol_init_agg_key,
						ethereum_deployment_block,
						TOTAL_ISSUANCE,
						DAILY_SLASHING_RATE,
						genesis_funding_amount,
						min_funding,
						REDEMPTION_TAX,
						EPOCH_DURATION_BLOCKS,
						REDEMPTION_TTL_SECS,
						CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
						BACKUP_NODE_EMISSION_INFLATION_PERBILL,
						EXPIRY_SPAN_IN_SECONDS,
						ACCRUAL_RATIO,
						Percent::from_percent(REDEMPTION_PERIOD_AS_PERCENTAGE),
						SUPPLY_UPDATE_INTERVAL,
						PENALTIES.to_vec(),
						KEYGEN_CEREMONY_TIMEOUT_BLOCKS,
						THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS,
						dot_runtime_version,
						hub_runtime_version,
						BITCOIN_EXPIRY_BLOCKS,
						ETHEREUM_EXPIRY_BLOCKS,
						ARBITRUM_EXPIRY_BLOCKS,
						POLKADOT_EXPIRY_BLOCKS,
						SOLANA_EXPIRY_BLOCKS,
						ASSETHUB_EXPIRY_BLOCKS,
						BITCOIN_SAFETY_MARGIN,
						ETHEREUM_SAFETY_MARGIN,
						ARBITRUM_SAFETY_MARGIN,
						SOLANA_SAFETY_MARGIN,
						AUCTION_BID_CUTOFF_PERCENTAGE,
						SolanaElectionsConfig {
							option_initial_state: Some(solana_elections::initial_state(
								sol_vault_program,
								sol_usdc_token_mint_pubkey,
								sol_swap_endpoint_program_data_account,
								SHARED_DATA_REFERENCE_LIFETIME,
							)),
						},
						BitcoinElectionsConfig {
							option_initial_state: Some(bitcoin_elections::initial_state()),
						},
						GenericElectionsConfig {
							option_initial_state: Some(generic_elections::initial_state(
								chainlink_oracle_price_settings.convert(ArrayToVector),
							)),
						},
					))
					.build())
			}
		}
	};
}

network_spec!(testnet);
network_spec!(sisyphos);
network_spec!(perseverance);
network_spec!(berghain);

/// Configure initial storage state for FRAME modules.
/// 150 authority limit
fn testnet_genesis(
	initial_authorities: Vec<(AccountId, AuraId, GrandpaId)>, // initial validators
	extra_accounts: Vec<(AccountId, AccountRole, u128, Option<Vec<u8>>)>,
	root_key: AccountId,
	min_authorities: AuthorityCount,
	auction_parameters: SetSizeParameters,
	max_authority_set_contraction_percentage: Percent,
	environment_genesis_config: state_chain_runtime::EnvironmentConfig,
	eth_init_agg_key: [u8; 33],
	sol_init_agg_key: Option<SolAddress>,
	ethereum_deployment_block: u64,
	total_issuance: FlipBalance,
	daily_slashing_rate: Permill,
	genesis_funding_amount: u128,
	minimum_funding: u128,
	redemption_tax: u128,
	epoch_duration: BlockNumber,
	redemption_ttl_secs: u64,
	current_authority_emission_inflation_perbill: u32,
	backup_node_emission_inflation_perbill: u32,
	expiry_span: u64,
	accrual_ratio: (i32, u32),
	redemption_period_as_percentage: Percent,
	supply_update_interval: u32,
	penalties: Vec<(Offence, (i32, BlockNumber))>,
	keygen_ceremony_timeout_blocks: BlockNumber,
	threshold_signature_ceremony_timeout_blocks: BlockNumber,
	dot_runtime_version: RuntimeVersion,
	hub_runtime_version: RuntimeVersion,
	bitcoin_deposit_channel_lifetime: u32,
	ethereum_deposit_channel_lifetime: u32,
	arbitrum_deposit_channel_lifetime: u32,
	polkadot_deposit_channel_lifetime: u32,
	solana_deposit_channel_lifetime: u32,
	assethub_deposit_channel_lifetime: u32,
	bitcoin_safety_margin: u64,
	ethereum_safety_margin: u64,
	arbitrum_safety_margin: u64,
	solana_safety_margin: u64,
	auction_bid_cutoff_percentage: Percent,
	solana_elections: state_chain_runtime::SolanaElectionsConfig,
	bitcoin_elections: state_chain_runtime::BitcoinElectionsConfig,
	generic_elections: state_chain_runtime::GenericElectionsConfig,
) -> serde_json::Value {
	// Sanity Checks
	for (account_id, aura_id, grandpa_id) in initial_authorities.iter() {
		assert_eq!(
			AsRef::<[u8]>::as_ref(account_id),
			AsRef::<[u8]>::as_ref(aura_id),
			"Aura and Account ID ({}) should be the same",
			account_id
		);
		assert_ne!(
			AsRef::<[u8]>::as_ref(grandpa_id),
			AsRef::<[u8]>::as_ref(aura_id),
			"Aura and Grandpa ID should be different for {}.",
			account_id
		);
	}

	let authority_ids: BTreeSet<AccountId> =
		initial_authorities.iter().map(|(id, ..)| id.clone()).collect();
	let (extra_accounts, genesis_vanity_names): (Vec<_>, BTreeMap<_, _>) = extra_accounts
		.into_iter()
		.map(|(account, role, balance, vanity)| {
			((account.clone(), role, balance), (account, vanity))
		})
		.unzip();
	let genesis_vanity_names = genesis_vanity_names
		.into_iter()
		.filter_map(|(account, vanity)| vanity.map(|vanity| (account, vanity)))
		.collect::<BTreeMap<_, _>>();
	let all_accounts: BTreeSet<_> = initial_authorities
		.iter()
		.filter_map(|(account_id, ..)| -> Option<(AccountId, AccountRole, u128)> {
			if let Some((_, role, funds)) = extra_accounts.iter().find(|(id, ..)| id == account_id)
			{
				// If the genesis account is listed in `extra_accounts` we will use the details from
				// there.
				assert!(*role == AccountRole::Validator, "Extra account is not a validator.");
				log::info!(
					"Using custom values for genesis authority {}: {} FLIP",
					account_id,
					funds / FLIPPERINOS_PER_FLIP
				);
				None
			} else {
				// Otherwise we will use the default values.
				log::info!(
					"Using default funds for genesis authority {}: {} FLIP",
					account_id,
					genesis_funding_amount / FLIPPERINOS_PER_FLIP
				);
				Some((account_id.clone(), AccountRole::Validator, genesis_funding_amount))
			}
		})
		.chain(extra_accounts.clone())
		.collect();

	assert!(
		genesis_vanity_names
			.keys()
			.all(|id| all_accounts.iter().any(|(acc_id, ..)| acc_id == id)),
		"Found a vanity name for non-genesis account."
	);

	let genesis_vanity_names = genesis_vanity_names
		.into_iter()
		.map(|(id, name)| BoundedVec::try_from(name).map(|bounded_name| (id, bounded_name)))
		.collect::<Result<BTreeMap<_, _>, _>>()
		.expect("Vanity names should be valid utf8 and within length bounds.");

	serde_json::to_value(state_chain_runtime::RuntimeGenesisConfig {
		account_roles: state_chain_runtime::AccountRolesConfig {
			initial_account_roles: all_accounts
				.iter()
				.map(|(id, role, ..)| (id.clone(), *role))
				.collect::<Vec<_>>(),
			genesis_vanity_names,
		},
		validator: state_chain_runtime::ValidatorConfig {
			genesis_authorities: authority_ids.clone(),
			genesis_backups: extra_accounts
				.iter()
				.filter_map(|(id, role, amount)| {
					if *role == AccountRole::Validator {
						Some((id.clone(), *amount))
					} else {
						None
					}
				})
				.collect::<_>(),
			epoch_duration,
			redemption_period_as_percentage,
			backup_reward_node_percentage: Percent::from_percent(33),
			bond: all_accounts
				.iter()
				.filter_map(|(id, _, funds)| authority_ids.contains(id).then_some(*funds))
				.min()
				.inspect(|bond| {
					log::info!("Bond will be set to {:?} Flip", bond / FLIPPERINOS_PER_FLIP);
				})
				.expect("At least one authority is required"),
			authority_set_min_size: min_authorities,
			auction_parameters,
			auction_bid_cutoff_percentage,
			max_authority_set_contraction_percentage,
		},
		session: state_chain_runtime::SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| (x.0.clone(), x.0.clone(), session_keys(x.1.clone(), x.2.clone())))
				.collect::<Vec<_>>(),
		},
		flip: state_chain_runtime::FlipConfig { total_issuance, daily_slashing_rate },
		funding: state_chain_runtime::FundingConfig {
			genesis_accounts: Vec::from_iter(all_accounts.clone())
				.into_iter()
				.map(|(id, _role, amount)| (id, amount))
				.collect::<Vec<_>>(),
			minimum_funding,
			redemption_tax,
			redemption_ttl: core::time::Duration::from_secs(redemption_ttl_secs),
		},
		// These are set indirectly via the session pallet.
		aura: state_chain_runtime::AuraConfig { authorities: vec![] },
		// These are set indirectly via the session pallet.
		grandpa: state_chain_runtime::GrandpaConfig { authorities: vec![], ..Default::default() },
		governance: state_chain_runtime::GovernanceConfig {
			members: BTreeSet::from([root_key]),
			expiry_span,
		},
		reputation: state_chain_runtime::ReputationConfig {
			accrual_ratio,
			penalties,
			genesis_validators: all_accounts
				.iter()
				.filter_map(
					|(id, role, _)| {
						if *role == AccountRole::Validator {
							Some(id.clone())
						} else {
							None
						}
					},
				)
				.collect::<_>(),
		},
		environment: environment_genesis_config,

		ethereum_vault: state_chain_runtime::EthereumVaultConfig {
			deployment_block: Some(ethereum_deployment_block),
			chain_initialized: true,
		},

		arbitrum_vault: state_chain_runtime::ArbitrumVaultConfig {
			deployment_block: None,
			chain_initialized: false,
		},

		solana_vault: state_chain_runtime::SolanaVaultConfig {
			deployment_block: None,
			chain_initialized: false,
		},

		assethub_vault: state_chain_runtime::AssethubVaultConfig {
			deployment_block: None,
			chain_initialized: false,
		},

		evm_threshold_signer: state_chain_runtime::EvmThresholdSignerConfig {
			key: Some(cf_chains::evm::AggKey::from_pubkey_compressed(eth_init_agg_key)),
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
			amount_to_slash: FLIPPERINOS_PER_FLIP,
			..Default::default()
		},
		polkadot_threshold_signer: state_chain_runtime::PolkadotThresholdSignerConfig {
			threshold_signature_response_timeout: threshold_signature_ceremony_timeout_blocks,
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
			amount_to_slash: FLIPPERINOS_PER_FLIP,
			..Default::default()
		},
		bitcoin_threshold_signer: state_chain_runtime::BitcoinThresholdSignerConfig {
			threshold_signature_response_timeout: threshold_signature_ceremony_timeout_blocks,
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
			amount_to_slash: FLIPPERINOS_PER_FLIP,
			..Default::default()
		},
		solana_threshold_signer: state_chain_runtime::SolanaThresholdSignerConfig {
			key: sol_init_agg_key,
			threshold_signature_response_timeout: threshold_signature_ceremony_timeout_blocks,
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
			amount_to_slash: FLIPPERINOS_PER_FLIP,
			..Default::default()
		},
		emissions: state_chain_runtime::EmissionsConfig {
			current_authority_emission_inflation: current_authority_emission_inflation_perbill,
			backup_node_emission_inflation: backup_node_emission_inflation_perbill,
			supply_update_interval,
			..Default::default()
		},
		// !!! These Chain tracking values should be set to reasonable values at time of launch !!!
		ethereum_chain_tracking: state_chain_runtime::EthereumChainTrackingConfig {
			init_chain_state: ChainState::<Ethereum> {
				block_height: 0,
				tracked_data: EthereumTrackedData {
					base_fee: 1000000u32.into(),
					priority_fee: 100u32.into(),
				},
			},
		},
		polkadot_chain_tracking: state_chain_runtime::PolkadotChainTrackingConfig {
			init_chain_state: ChainState::<Polkadot> {
				block_height: 0,
				tracked_data: PolkadotTrackedData {
					median_tip: 0,
					runtime_version: dot_runtime_version,
				},
			},
		},
		bitcoin_chain_tracking: state_chain_runtime::BitcoinChainTrackingConfig {
			init_chain_state: ChainState::<Bitcoin> {
				block_height: 0,
				tracked_data: BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(1000) },
			},
		},
		arbitrum_chain_tracking: state_chain_runtime::ArbitrumChainTrackingConfig {
			init_chain_state: ChainState::<Arbitrum> {
				block_height: 0,
				tracked_data: ArbitrumTrackedData {
					base_fee: 100000000u32.into(),
					l1_base_fee_estimate: 1u128,
				},
			},
		},
		solana_chain_tracking: state_chain_runtime::SolanaChainTrackingConfig {
			init_chain_state: ChainState::<Solana> {
				block_height: 0,
				tracked_data: SolTrackedData { priority_fee: 100_000 },
			},
		},
		assethub_chain_tracking: state_chain_runtime::AssethubChainTrackingConfig {
			init_chain_state: ChainState::<Assethub> {
				block_height: 0,
				tracked_data: AssethubTrackedData {
					median_tip: 0,
					runtime_version: hub_runtime_version,
				},
			},
		},
		// Channel lifetimes are set to ~2 hours at average block times.
		bitcoin_ingress_egress: state_chain_runtime::BitcoinIngressEgressConfig {
			deposit_channel_lifetime: bitcoin_deposit_channel_lifetime.into(),
			witness_safety_margin: Some(bitcoin_safety_margin),
			dust_limits: vec![(btc::Asset::Btc, BITCOIN_DUST_LIMIT)],
		},
		ethereum_ingress_egress: state_chain_runtime::EthereumIngressEgressConfig {
			deposit_channel_lifetime: ethereum_deposit_channel_lifetime.into(),
			witness_safety_margin: Some(ethereum_safety_margin),
			..Default::default()
		},
		polkadot_ingress_egress: state_chain_runtime::PolkadotIngressEgressConfig {
			deposit_channel_lifetime: polkadot_deposit_channel_lifetime,
			..Default::default()
		},
		arbitrum_ingress_egress: state_chain_runtime::ArbitrumIngressEgressConfig {
			deposit_channel_lifetime: arbitrum_deposit_channel_lifetime.into(),
			witness_safety_margin: Some(arbitrum_safety_margin),
			..Default::default()
		},
		solana_ingress_egress: state_chain_runtime::SolanaIngressEgressConfig {
			deposit_channel_lifetime: solana_deposit_channel_lifetime as u64,
			witness_safety_margin: Some(solana_safety_margin),
			..Default::default()
		},
		assethub_ingress_egress: state_chain_runtime::AssethubIngressEgressConfig {
			deposit_channel_lifetime: assethub_deposit_channel_lifetime,
			..Default::default()
		},
		solana_elections,

		// TODO: Set correct initial state
		bitcoin_elections,

		generic_elections,

		// We can't use ..Default::default() here because chain tracking panics on default (by
		// design). And the way ..Default::default() syntax works is that it generates the default
		// value for the whole struct, not just the fields that are missing.
		swapping: Default::default(),
		bitcoin_vault: Default::default(),
		polkadot_vault: Default::default(),
		system: Default::default(),
		transaction_payment: Default::default(),

		// instance1
		ethereum_broadcaster: state_chain_runtime::EthereumBroadcasterConfig {
			broadcast_timeout: 5 * BLOCKS_PER_MINUTE_ETHEREUM,
		},
		// instance2
		polkadot_broadcaster: state_chain_runtime::PolkadotBroadcasterConfig {
			broadcast_timeout: 4 * BLOCKS_PER_MINUTE_POLKADOT,
		},
		// instance3
		bitcoin_broadcaster: state_chain_runtime::BitcoinBroadcasterConfig {
			broadcast_timeout: 9, // = 90 minutes
		},
		// instance 4
		arbitrum_broadcaster: state_chain_runtime::ArbitrumBroadcasterConfig {
			broadcast_timeout: 2 * BLOCKS_PER_MINUTE_ARBITRUM,
		},
		// instance 5
		solana_broadcaster: state_chain_runtime::SolanaBroadcasterConfig {
			broadcast_timeout: 4 * BLOCKS_PER_MINUTE_SOLANA,
		},
		// instance6
		assethub_broadcaster: state_chain_runtime::AssethubBroadcasterConfig {
			broadcast_timeout: 4 * BLOCKS_PER_MINUTE_ASSETHUB,
		},
	})
	.expect("Genesis config is JSON-compatible.")
}

pub fn chainflip_properties() -> Properties {
	json!({
		"ss58Format": state_chain_runtime::constants::common::CHAINFLIP_SS58_PREFIX,
		"tokenDecimals": 18,
		"tokenSymbol": "FLIP",
		"color": "#61CFAA",
	})
	.as_object()
	.unwrap()
	.clone()
}

/// Sets global that ensures SC AccountId's are printed correctly
pub fn use_chainflip_account_id_encoding() {
	set_default_ss58_version(Ss58AddressFormat::custom(common::CHAINFLIP_SS58_PREFIX));
}

#[test]
fn can_build_genesis() {
	use_chainflip_account_id_encoding();
	let _ = testnet::Config::build_spec(None).unwrap();
	let _ = sisyphos::Config::build_spec(None).unwrap();
	let _ = perseverance::Config::build_spec(None).unwrap();
	let _ = berghain::Config::build_spec(None).unwrap();
}
