use cf_chains::{
	btc::BitcoinNetwork,
	dot::{PolkadotHash, RuntimeVersion},
};
use cf_primitives::{AccountRole, AuthorityCount, PolkadotAccountId};

use common::FLIPPERINOS_PER_FLIP;
use frame_benchmarking::sp_std::collections::btree_set::BTreeSet;
use sc_service::{ChainType, Properties};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{
	crypto::{set_default_ss58_version, Ss58AddressFormat, UncheckedInto},
	sr25519, Pair, Public,
};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::{
	chainflip::Offence, opaque::SessionKeys, AccountId, AccountRolesConfig, AuraConfig,
	BitcoinThresholdSignerConfig, BitcoinVaultConfig, BlockNumber, CfeSettings, EmissionsConfig,
	EnvironmentConfig, EthereumThresholdSignerConfig, EthereumVaultConfig, FlipBalance, FlipConfig,
	GenesisConfig, GovernanceConfig, GrandpaConfig, PolkadotThresholdSignerConfig,
	PolkadotVaultConfig, ReputationConfig, SessionConfig, Signature, StakingConfig, SystemConfig,
	ValidatorConfig, WASM_BINARY,
};

use std::{env, marker::PhantomData, str::FromStr};
use utils::clean_eth_address;

use sp_runtime::traits::{IdentifyAccount, Verify};

pub mod common;
pub mod sisyphos;
pub mod testnet;

/// Generate a crypto pair from seed.
pub fn test_account_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{seed}"), None)
		.expect("static values are valid; qed")
		.public()
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
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig>;

const BITCOIN_FEE_PER_UTXO: u64 = 1000; // Todo: what value to put here?

/// generate session keys from Aura and Grandpa keys
pub fn session_keys(aura: AuraId, grandpa: GrandpaId) -> SessionKeys {
	SessionKeys { aura, grandpa }
}
pub struct StateChainEnvironment {
	flip_token_address: [u8; 20],
	eth_usdc_address: [u8; 20],
	stake_manager_address: [u8; 20],
	key_manager_address: [u8; 20],
	eth_vault_address: [u8; 20],
	ethereum_chain_id: u64,
	eth_init_agg_key: [u8; 33],
	ethereum_deployment_block: u64,
	genesis_stake_amount: u128,
	/// Note: Minimum stake should be expressed in Flipperinos.
	min_stake: u128,
	// CFE config values starts here
	eth_block_safety_margin: u32,
	max_ceremony_stage_duration: u32,

	dot_genesis_hash: PolkadotHash,
	dot_vault_account_id: Option<PolkadotAccountId>,
	dot_runtime_version: RuntimeVersion,
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
	from_env_var!(clean_eth_address, FLIP_TOKEN_ADDRESS, flip_token_address);
	from_env_var!(clean_eth_address, ETH_USDC_ADDRESS, eth_usdc_address);
	from_env_var!(clean_eth_address, STAKE_MANAGER_ADDRESS, stake_manager_address);
	from_env_var!(clean_eth_address, KEY_MANAGER_ADDRESS, key_manager_address);
	from_env_var!(clean_eth_address, ETH_VAULT_ADDRESS, eth_vault_address);
	from_env_var!(hex_decode, ETH_INIT_AGG_KEY, eth_init_agg_key);
	from_env_var!(FromStr::from_str, ETHEREUM_CHAIN_ID, ethereum_chain_id);
	from_env_var!(FromStr::from_str, ETH_DEPLOYMENT_BLOCK, ethereum_deployment_block);
	from_env_var!(FromStr::from_str, GENESIS_STAKE, genesis_stake_amount);
	from_env_var!(FromStr::from_str, ETH_BLOCK_SAFETY_MARGIN, eth_block_safety_margin);
	from_env_var!(FromStr::from_str, MAX_CEREMONY_STAGE_DURATION, max_ceremony_stage_duration);
	from_env_var!(FromStr::from_str, MIN_STAKE, min_stake);

	let dot_genesis_hash = match env::var("DOT_GENESIS_HASH") {
		Ok(s) => hex_decode::<32>(&s).unwrap().into(),
		Err(_) => defaults.dot_genesis_hash,
	};
	let dot_vault_account_id = match env::var("DOT_VAULT_ACCOUNT_ID") {
		Ok(s) => Some(hex_decode::<32>(&s).unwrap().into()),
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

	StateChainEnvironment {
		flip_token_address,
		eth_usdc_address,
		stake_manager_address,
		key_manager_address,
		eth_vault_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		max_ceremony_stage_duration,
		min_stake,
		dot_genesis_hash,
		dot_vault_account_id,
		dot_runtime_version: RuntimeVersion {
			spec_version: dot_spec_version,
			transaction_version: dot_transaction_version,
		},
	}
}

/// Start a single node development chain - using bashful as genesis node
pub fn cf_development_config() -> Result<ChainSpec, String> {
	let wasm_binary =
		WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;

	let snow_white =
		hex_literal::hex!["ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"];
	let bashful_sr25519 =
		hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
	let StateChainEnvironment {
		flip_token_address,
		eth_usdc_address,
		stake_manager_address,
		key_manager_address,
		eth_vault_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		max_ceremony_stage_duration,
		min_stake,
		dot_genesis_hash,
		dot_vault_account_id,
		dot_runtime_version,
	} = get_environment_or_defaults(testnet::ENV);
	Ok(ChainSpec::from_genesis(
		"CF Develop",
		"cf-dev",
		ChainType::Development,
		move || {
			testnet_genesis(
				wasm_binary,
				// Initial PoA authorities
				vec![(
					// Bashful
					bashful_sr25519.into(),
					bashful_sr25519.unchecked_into(),
					hex_literal::hex![
						"971b584324592e9977f0ae407eb6b8a1aa5bcd1ca488e54ab49346566f060dd8"
					]
					.unchecked_into(),
				)],
				// Extra accounts
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("LP_1"),
						AccountRole::LiquidityProvider,
						100 * FLIPPERINOS_PER_FLIP,
					),
					(
						get_account_id_from_seed::<sr25519::Public>("LP_2"),
						AccountRole::LiquidityProvider,
						100 * FLIPPERINOS_PER_FLIP,
					),
					(
						get_account_id_from_seed::<sr25519::Public>("RELAYER_1"),
						AccountRole::Relayer,
						100 * FLIPPERINOS_PER_FLIP,
					),
					(
						get_account_id_from_seed::<sr25519::Public>("RELAYER_2"),
						AccountRole::Relayer,
						100 * FLIPPERINOS_PER_FLIP,
					),
				],
				// Governance account - Snow White
				snow_white.into(),
				1,
				common::MAX_AUTHORITIES,
				EnvironmentConfig {
					flip_token_address,
					eth_usdc_address,
					stake_manager_address,
					key_manager_address,
					eth_vault_address,
					ethereum_chain_id,
					cfe_settings: CfeSettings {
						eth_block_safety_margin,
						max_ceremony_stage_duration,
						eth_priority_fee_percentile: common::ETH_PRIORITY_FEE_PERCENTILE,
					},

					polkadot_genesis_hash: dot_genesis_hash,
					polkadot_vault_account_id: dot_vault_account_id.clone(),
					polkadot_runtime_version: dot_runtime_version,
					bitcoin_network: BitcoinNetwork::Regtest,
					bitcoin_fee_per_utxo: BITCOIN_FEE_PER_UTXO,
				},
				eth_init_agg_key,
				ethereum_deployment_block,
				common::TOTAL_ISSUANCE,
				genesis_stake_amount,
				min_stake,
				8 * common::HOURS,
				common::CLAIM_DELAY_SECS,
				common::CLAIM_DELAY_BUFFER_SECS,
				common::CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
				common::BACKUP_NODE_EMISSION_INFLATION_PERBILL,
				common::EXPIRY_SPAN_IN_SECONDS,
				common::ACCRUAL_RATIO,
				common::PERCENT_OF_EPOCH_PERIOD_CLAIMABLE,
				common::SUPPLY_UPDATE_INTERVAL,
				common::PENALTIES.to_vec(),
				common::KEYGEN_CEREMONY_TIMEOUT_BLOCKS,
				common::THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Fork ID
		None,
		// Properties
		None,
		// Extensions
		None,
	))
}

macro_rules! network_spec {
	( $network:ident ) => {
		impl $network::Config {
			pub fn build_spec(
				env_override: Option<StateChainEnvironment>,
			) -> Result<ChainSpec, String> {
				use $network::*;

				let wasm_binary =
					WASM_BINARY.ok_or_else(|| "Wasm binary not available".to_string())?;
				let StateChainEnvironment {
					flip_token_address,
					eth_usdc_address,
					stake_manager_address,
					key_manager_address,
					eth_vault_address,
					ethereum_chain_id,
					eth_init_agg_key,
					ethereum_deployment_block,
					genesis_stake_amount,
					eth_block_safety_margin,
					max_ceremony_stage_duration,
					min_stake,
					dot_genesis_hash,
					dot_vault_account_id,
					dot_runtime_version,
				} = env_override.unwrap_or(ENV);
				Ok(ChainSpec::from_genesis(
					NETWORK_NAME,
					NETWORK_NAME,
					CHAIN_TYPE,
					move || {
						testnet_genesis(
							wasm_binary,
							// Initial PoA authorities
							vec![
								(
									BASHFUL_SR25519.into(),
									BASHFUL_SR25519.unchecked_into(),
									BASHFUL_ED25519.unchecked_into(),
								),
								(
									DOC_SR25519.into(),
									DOC_SR25519.unchecked_into(),
									DOC_ED25519.unchecked_into(),
								),
								(
									DOPEY_SR25519.into(),
									DOPEY_SR25519.unchecked_into(),
									DOPEY_ED25519.unchecked_into(),
								),
							],
							// Extra accounts
							vec![
								(
									get_account_id_from_seed::<sr25519::Public>("LP_1"),
									AccountRole::LiquidityProvider,
									100 * FLIPPERINOS_PER_FLIP,
								),
								(
									get_account_id_from_seed::<sr25519::Public>("LP_2"),
									AccountRole::LiquidityProvider,
									100 * FLIPPERINOS_PER_FLIP,
								),
								(
									get_account_id_from_seed::<sr25519::Public>("RELAYER_1"),
									AccountRole::Relayer,
									100 * FLIPPERINOS_PER_FLIP,
								),
								(
									get_account_id_from_seed::<sr25519::Public>("RELAYER_2"),
									AccountRole::Relayer,
									100 * FLIPPERINOS_PER_FLIP,
								),
							],
							// Governance account - Snow White
							SNOW_WHITE_SR25519.into(),
							MIN_AUTHORITIES,
							MAX_AUTHORITIES,
							EnvironmentConfig {
								flip_token_address,
								eth_usdc_address,
								stake_manager_address,
								key_manager_address,
								eth_vault_address,
								ethereum_chain_id,
								cfe_settings: CfeSettings {
									eth_block_safety_margin,
									max_ceremony_stage_duration,
									eth_priority_fee_percentile: ETH_PRIORITY_FEE_PERCENTILE,
								},

								polkadot_genesis_hash: dot_genesis_hash,
								polkadot_vault_account_id: dot_vault_account_id.clone(),
								polkadot_runtime_version: dot_runtime_version,
								bitcoin_network: BitcoinNetwork::Regtest,
								bitcoin_fee_per_utxo: BITCOIN_FEE_PER_UTXO,
							},
							eth_init_agg_key,
							ethereum_deployment_block,
							TOTAL_ISSUANCE,
							genesis_stake_amount,
							min_stake,
							3 * HOURS,
							CLAIM_DELAY_SECS,
							CLAIM_DELAY_BUFFER_SECS,
							CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
							BACKUP_NODE_EMISSION_INFLATION_PERBILL,
							EXPIRY_SPAN_IN_SECONDS,
							ACCRUAL_RATIO,
							PERCENT_OF_EPOCH_PERIOD_CLAIMABLE,
							SUPPLY_UPDATE_INTERVAL,
							PENALTIES.to_vec(),
							KEYGEN_CEREMONY_TIMEOUT_BLOCKS,
							THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS,
						)
					},
					// Bootnodes
					vec![],
					// Telemetry
					None,
					// Protocol ID
					None,
					// Fork ID
					None,
					// Properties
					Some(chainflip_properties()),
					// Extensions
					None,
				))
			}
		}
	};
}

network_spec!(testnet);
network_spec!(sisyphos);

/// Configure initial storage state for FRAME modules.
/// 150 authority limit
#[allow(clippy::too_many_arguments)]
fn testnet_genesis(
	wasm_binary: &[u8],
	initial_authorities: Vec<(AccountId, AuraId, GrandpaId)>, // initial validators
	extra_stakers: Vec<(AccountId, AccountRole, u128)>,
	root_key: AccountId,
	min_authorities: AuthorityCount,
	max_authorities: AuthorityCount,
	config_set: EnvironmentConfig,
	eth_init_agg_key: [u8; 33],
	ethereum_deployment_block: u64,
	total_issuance: FlipBalance,
	genesis_stake_amount: u128,
	minimum_stake: u128,
	blocks_per_epoch: BlockNumber,
	claim_delay: u64,
	claim_delay_buffer_seconds: u64,
	current_authority_emission_inflation_perbill: u32,
	backup_node_emission_inflation_perbill: u32,
	expiry_span: u64,
	accrual_ratio: (i32, u32),
	percent_of_epoch_period_claimable: u8,
	supply_update_interval: u32,
	penalties: Vec<(Offence, (i32, BlockNumber))>,
	keygen_ceremony_timeout_blocks: BlockNumber,
	threshold_signature_ceremony_timeout_blocks: BlockNumber,
) -> GenesisConfig {
	let authority_ids: Vec<AccountId> =
		initial_authorities.iter().map(|(id, ..)| id.clone()).collect();
	let total_issuance =
		total_issuance + extra_stakers.iter().map(|(_, _, stake)| *stake).sum::<u128>();
	let all_accounts: Vec<_> = initial_authorities
		.iter()
		.map(|(account_id, ..)| (account_id.clone(), AccountRole::Validator, genesis_stake_amount))
		.chain(extra_stakers.clone())
		.collect();

	GenesisConfig {
		account_roles: AccountRolesConfig {
			initial_account_roles: all_accounts
				.iter()
				.map(|(id, role, ..)| (id.clone(), *role))
				.collect(),
		},
		system: SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
		},
		validator: ValidatorConfig {
			genesis_authorities: authority_ids,
			genesis_backups: extra_stakers
				.iter()
				.filter_map(|(id, role, stake)| {
					if *role == AccountRole::Validator {
						Some((id.clone(), *stake))
					} else {
						None
					}
				})
				.collect(),
			blocks_per_epoch,
			claim_period_as_percentage: percent_of_epoch_period_claimable,
			backup_reward_node_percentage: 20,
			bond: genesis_stake_amount,
			authority_set_min_size: min_authorities,
			min_size: min_authorities,
			max_size: max_authorities,
			max_expansion: max_authorities,
		},
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| (x.0.clone(), x.0.clone(), session_keys(x.1.clone(), x.2.clone())))
				.collect::<Vec<_>>(),
		},
		flip: FlipConfig { total_issuance },
		staking: StakingConfig {
			genesis_stakers: all_accounts
				.iter()
				.map(|(acct, _role, stake)| (acct.clone(), *stake))
				.collect(),
			minimum_stake,
			claim_ttl: core::time::Duration::from_secs(3 * claim_delay),
			claim_delay_buffer_seconds,
		},
		aura: AuraConfig { authorities: vec![] },
		grandpa: GrandpaConfig { authorities: vec![] },
		governance: GovernanceConfig { members: BTreeSet::from([root_key]), expiry_span },
		reputation: ReputationConfig {
			accrual_ratio,
			penalties,
			// Includes backups.
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
				.collect(),
		},
		environment: config_set,
		ethereum_vault: EthereumVaultConfig {
			vault_key: Some(eth_init_agg_key.to_vec()),
			deployment_block: ethereum_deployment_block,
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
		},

		polkadot_vault: PolkadotVaultConfig {
			vault_key: None,
			deployment_block: 0,
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
		},
		bitcoin_vault: BitcoinVaultConfig {
			vault_key: None,
			deployment_block: 0,
			keygen_response_timeout: keygen_ceremony_timeout_blocks,
		},
		ethereum_threshold_signer: EthereumThresholdSignerConfig {
			threshold_signature_response_timeout: threshold_signature_ceremony_timeout_blocks,
			_instance: PhantomData,
		},

		polkadot_threshold_signer: PolkadotThresholdSignerConfig {
			threshold_signature_response_timeout: threshold_signature_ceremony_timeout_blocks,
			_instance: PhantomData,
		},
		bitcoin_threshold_signer: BitcoinThresholdSignerConfig {
			threshold_signature_response_timeout: threshold_signature_ceremony_timeout_blocks,
			_instance: PhantomData,
		},
		emissions: EmissionsConfig {
			current_authority_emission_inflation: current_authority_emission_inflation_perbill,
			backup_node_emission_inflation: backup_node_emission_inflation_perbill,
			supply_update_interval,
		},
		transaction_payment: Default::default(),
		liquidity_pools: Default::default(),
	}
}

pub fn chainflip_properties() -> Properties {
	let mut properties = Properties::new();
	properties.insert(
		"ss58Format".into(),
		state_chain_runtime::constants::common::CHAINFLIP_SS58_PREFIX.into(),
	);
	properties.insert("tokenDecimals".into(), 18.into());
	properties.insert("tokenSymbol".into(), "FLIP".into());
	properties.insert("color".into(), "#61CFAA".into());

	properties
}

/// Sets global that ensures SC AccountId's are printed correctly
pub fn use_chainflip_account_id_encoding() {
	set_default_ss58_version(Ss58AddressFormat::custom(common::CHAINFLIP_SS58_PREFIX));
}
