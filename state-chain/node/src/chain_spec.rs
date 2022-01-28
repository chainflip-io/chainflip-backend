use sc_service::{ChainType, Properties};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::UncheckedInto, sr25519, Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::traits::{IdentifyAccount, Verify};
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, AuctionConfig, AuraConfig,
	EmissionsConfig, EnvironmentConfig, FlipBalance, FlipConfig, GenesisConfig, GovernanceConfig,
	GrandpaConfig, ReputationConfig, SessionConfig, Signature, StakingConfig, SystemConfig,
	ValidatorConfig, VaultsConfig, WASM_BINARY,
};
use std::{convert::TryInto, env};
use utilities::clean_eth_address;

mod network_env;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig>;

const STAKE_MANAGER_ADDRESS_DEFAULT: &str = "9Dfaa29bEc7d22ee01D533Ebe8faA2be5799C77F";
const KEY_MANAGER_ADDRESS_DEFAULT: &str = "36fB9E46D6cBC14600D9089FD7Ce95bCf664179f";
const ETHEREUM_CHAIN_ID_DEFAULT: u64 = cf_chains::eth::CHAIN_ID_RINKEBY;
const ETH_INIT_AGG_KEY_DEFAULT: &str =
	"02e61afd677cdfbec838c6f309deff0b2c6056f8a27f2c783b68bba6b30f667be6";
// 50k FLIP in Fliperinos
const GENESIS_STAKE_AMOUNT_DEFAULT: FlipBalance = 50_000_000_000_000_000_000_000;
const ETH_DEPLOYMENT_BLOCK_DEFAULT: u64 = 0;

// CFE config default values
const ETH_BLOCK_SAFETY_MARGIN_DEFAULT: u32 = 4;
pub const MAX_EXTRINSIC_RETRY_ATTEMPTS_DEFAULT: u32 = 10;
const MAX_STAGE_DURATION_SECS_DEFAULT: u32 = 300;
const PENDING_SIGN_DURATION_SECS_DEFAULT: u32 = 500;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// generate session keys from Aura and Grandpa keys
pub fn session_keys(aura: AuraId, grandpa: GrandpaId) -> SessionKeys {
	SessionKeys { aura, grandpa }
}

pub struct StateChainEnvironment {
	stake_manager_address: [u8; 20],
	key_manager_address: [u8; 20],
	ethereum_chain_id: u64,
	eth_init_agg_key: [u8; 33],
	ethereum_deployment_block: u64,
	genesis_stake_amount: u128,
	// CFE config values starts here
	eth_block_safety_margin: u32,
	pending_sign_duration_secs: u32,
	max_ceremony_stage_duration_secs: u32,
	max_extrinsic_retry_attempts: u32,
}
/// Get the values from the State Chain's environment variables. Else set them via the defaults
pub fn get_environment() -> StateChainEnvironment {
	let stake_manager_address: [u8; 20] = clean_eth_address(
		&env::var("STAKE_MANAGER_ADDRESS").unwrap_or(String::from(STAKE_MANAGER_ADDRESS_DEFAULT)),
	)
	.unwrap();
	let key_manager_address: [u8; 20] = clean_eth_address(
		&env::var("KEY_MANAGER_ADDRESS").unwrap_or(String::from(KEY_MANAGER_ADDRESS_DEFAULT)),
	)
	.unwrap();
	let ethereum_chain_id = env::var("ETHEREUM_CHAIN_ID")
		.unwrap_or(ETHEREUM_CHAIN_ID_DEFAULT.to_string())
		.parse::<u64>()
		.expect("ETHEREUM_CHAIN_ID env var could not be parsed to u64");
	let eth_init_agg_key =
		hex::decode(env::var("ETH_INIT_AGG_KEY").unwrap_or(String::from(ETH_INIT_AGG_KEY_DEFAULT)))
			.unwrap()
			.try_into()
			.expect("ETH_INIT_AGG_KEY Cast to agg pub key failed");

	let ethereum_deployment_block = env::var("ETH_DEPLOYMENT_BLOCK")
		.unwrap_or(format!("{}", ETH_DEPLOYMENT_BLOCK_DEFAULT))
		.parse::<u64>()
		.expect("ETH_DEPLOYMENT_BLOCK env var could not be parsed to u64");

	let genesis_stake_amount = env::var("GENESIS_STAKE")
		.unwrap_or(format!("{}", GENESIS_STAKE_AMOUNT_DEFAULT))
		.parse::<u128>()
		.expect("GENESIS_STAKE env var could not be parsed to u128");

	let eth_block_safety_margin = env::var("ETH_BLOCK_SAFETY_MARGIN")
		.unwrap_or(format!("{}", ETH_BLOCK_SAFETY_MARGIN_DEFAULT))
		.parse::<u32>()
		.expect("ETH_BLOCK_SAFETY_MARGIN env var could not be parsed to u32");

	let max_extrinsic_retry_attempts = env::var("MAX_EXTRINSIC_RETRY_ATTEMPTS")
		.unwrap_or(format!("{}", MAX_EXTRINSIC_RETRY_ATTEMPTS_DEFAULT))
		.parse::<u32>()
		.expect("MAX_EXTRINSIC_RETRY_ATTEMPTS env var could not be parsed to u32");

	let max_ceremony_stage_duration_secs = env::var("MAX_CEREMONY_STAGE_DURATION")
		.unwrap_or(format!("{}", MAX_STAGE_DURATION_SECS_DEFAULT))
		.parse::<u32>()
		.expect("MAX_CEREMONY_STAGE_DURATION env var could not be parsed to u32");

	let pending_sign_duration_secs = env::var("PENDING_SIGN_DURATION")
		.unwrap_or(format!("{}", PENDING_SIGN_DURATION_SECS_DEFAULT))
		.parse::<u32>()
		.expect("PENDING_SIGN_DURATION env var could not be parsed to u32");

	StateChainEnvironment {
		stake_manager_address,
		key_manager_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		pending_sign_duration_secs,
		max_ceremony_stage_duration_secs,
		max_extrinsic_retry_attempts,
	}
}

/// Generate an Aura authority key.
pub fn authority_keys_from_seed(s: &str) -> (AccountId, AuraId, GrandpaId) {
	(
		get_account_id_from_seed::<sr25519::Public>(s),
		get_from_seed::<AuraId>(s),
		get_from_seed::<GrandpaId>(s),
	)
}

/// Start a single node development chain
pub fn development_config() -> Result<ChainSpec, String> {
	let wasm_binary =
		WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;
	let StateChainEnvironment {
		stake_manager_address,
		key_manager_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		pending_sign_duration_secs,
		max_ceremony_stage_duration_secs,
		max_extrinsic_retry_attempts,
	} = get_environment();
	Ok(ChainSpec::from_genesis(
		"Develop",
		"dev",
		ChainType::Development,
		move || {
			testnet_genesis(
				wasm_binary,
				// Initial PoA authorities
				vec![authority_keys_from_seed("Alice")],
				// Governance account
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				// Stakers at genesis
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
				1,
				EnvironmentConfig {
					stake_manager_address,
					key_manager_address,
					ethereum_chain_id,
					eth_block_safety_margin,
					pending_sign_duration_secs,
					max_ceremony_stage_duration_secs,
					max_extrinsic_retry_attempts,
				},
				eth_init_agg_key,
				ethereum_deployment_block,
				genesis_stake_amount,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Properties
		None,
		// Extensions
		None,
	))
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
		stake_manager_address,
		key_manager_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		pending_sign_duration_secs,
		max_ceremony_stage_duration_secs,
		max_extrinsic_retry_attempts,
	} = get_environment();
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
				// Governance account - Snow White
				snow_white.into(),
				// Stakers at genesis
				vec![
					// Bashful
					bashful_sr25519.into(),
				],
				1,
				EnvironmentConfig {
					stake_manager_address,
					key_manager_address,
					ethereum_chain_id,
					eth_block_safety_margin,
					pending_sign_duration_secs,
					max_ceremony_stage_duration_secs,
					max_extrinsic_retry_attempts,
				},
				eth_init_agg_key,
				ethereum_deployment_block,
				genesis_stake_amount,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Properties
		None,
		// Extensions
		None,
	))
}

/// Initialise a Chainflip three-node testnet from the environment.
pub fn chainflip_three_node_testnet_config() -> Result<ChainSpec, String> {
	chainflip_three_node_testnet_config_from_env(
		"Three node testnet",
		"three-node-testnet",
		ChainType::Local,
		get_environment(),
	)
}

/// Build the chainspec for Soundcheck public testnet.
pub fn chainflip_soundcheck_config() -> Result<ChainSpec, String> {
	chainflip_three_node_testnet_config_from_env(
		"Chainflip Soundcheck",
		"soundcheck",
		ChainType::Live,
		network_env::SOUNDCHECK,
	)
}

fn chainflip_three_node_testnet_config_from_env(
	name: &str,
	id: &str,
	chain_type: ChainType,
	environment: StateChainEnvironment,
) -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Wasm binary not available".to_string())?;
	let bashful_sr25519 =
		hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
	let doc_sr25519 =
		hex_literal::hex!["8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04"];
	let dopey_sr25519 =
		hex_literal::hex!["ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e"];
	let snow_white =
		hex_literal::hex!["ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"];
	let StateChainEnvironment {
		stake_manager_address,
		key_manager_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		pending_sign_duration_secs,
		max_ceremony_stage_duration_secs,
		max_extrinsic_retry_attempts,
	} = environment;
	Ok(ChainSpec::from_genesis(
		name,
		id,
		chain_type,
		move || {
			testnet_genesis(
				wasm_binary,
				// Initial PoA authorities
				vec![
					(
						// Bashful
						bashful_sr25519.into(),
						bashful_sr25519.unchecked_into(),
						hex_literal::hex![
							"971b584324592e9977f0ae407eb6b8a1aa5bcd1ca488e54ab49346566f060dd8"
						]
						.unchecked_into(),
					),
					(
						// Doc
						doc_sr25519.into(),
						doc_sr25519.unchecked_into(),
						hex_literal::hex![
							"e4c4009bd437cba06a2f25cf02f4efc0cac4525193a88fe1d29196e5d0ff54e8"
						]
						.unchecked_into(),
					),
					(
						// Dopey
						dopey_sr25519.into(),
						dopey_sr25519.unchecked_into(),
						hex_literal::hex![
							"5506333c28f3dd39095696362194f69893bc24e3ec553dbff106cdcbfe1beea4"
						]
						.unchecked_into(),
					),
				],
				// Governance account - Snow White
				snow_white.into(),
				// Stakers at genesis
				vec![
					// Bashful
					bashful_sr25519.into(),
					// Doc
					doc_sr25519.into(),
					// Dopey
					dopey_sr25519.into(),
				],
				2,
				EnvironmentConfig {
					stake_manager_address,
					key_manager_address,
					ethereum_chain_id,
					eth_block_safety_margin,
					pending_sign_duration_secs,
					max_ceremony_stage_duration_secs,
					max_extrinsic_retry_attempts,
				},
				eth_init_agg_key,
				ethereum_deployment_block,
				genesis_stake_amount,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Properties
		Some(chainflip_properties()),
		// Extensions
		None,
	))
}

/// Initialise a Chainflip testnet
pub fn chainflip_testnet_config() -> Result<ChainSpec, String> {
	let wasm_binary =
		WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;
	let bashful_sr25519 =
		hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
	let doc_sr25519 =
		hex_literal::hex!["8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04"];
	let dopey_sr25519 =
		hex_literal::hex!["ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e"];
	let grumpy_sr25519 =
		hex_literal::hex!["28b5f5f1654393975f58e78cf06b6f3ab509b3629b0a4b08aaa3dce6bf6af805"];
	let happy_sr25519 =
		hex_literal::hex!["7e6eb0b15c1767360fdad63d6ff78a97374355b00b4d3511a522b1a8688a661d"];
	let snow_white =
		hex_literal::hex!["ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"];
	let StateChainEnvironment {
		stake_manager_address,
		key_manager_address,
		ethereum_chain_id,
		eth_init_agg_key,
		ethereum_deployment_block,
		genesis_stake_amount,
		eth_block_safety_margin,
		pending_sign_duration_secs,
		max_ceremony_stage_duration_secs,
		max_extrinsic_retry_attempts,
	} = get_environment();
	Ok(ChainSpec::from_genesis(
		"Internal testnet",
		"test",
		ChainType::Local,
		move || {
			testnet_genesis(
				wasm_binary,
				// Initial PoA authorities
				vec![
					(
						// Bashful
						bashful_sr25519.into(),
						bashful_sr25519.unchecked_into(),
						hex_literal::hex![
							"971b584324592e9977f0ae407eb6b8a1aa5bcd1ca488e54ab49346566f060dd8"
						]
						.unchecked_into(),
					),
					(
						// Doc
						doc_sr25519.into(),
						doc_sr25519.unchecked_into(),
						hex_literal::hex![
							"e4c4009bd437cba06a2f25cf02f4efc0cac4525193a88fe1d29196e5d0ff54e8"
						]
						.unchecked_into(),
					),
					(
						// Dopey
						dopey_sr25519.into(),
						dopey_sr25519.unchecked_into(),
						hex_literal::hex![
							"5506333c28f3dd39095696362194f69893bc24e3ec553dbff106cdcbfe1beea4"
						]
						.unchecked_into(),
					),
					(
						// Grumpy
						grumpy_sr25519.into(),
						grumpy_sr25519.unchecked_into(),
						hex_literal::hex![
							"b9036620f103cce552edbdd15e54810c6c3906975f042e3ff949af075636007f"
						]
						.unchecked_into(),
					),
					(
						// Happy
						happy_sr25519.into(),
						happy_sr25519.unchecked_into(),
						hex_literal::hex![
							"0bb5e73112e716dc54541e87d2287f2252fd479f166969dc37c07a504000dae9"
						]
						.unchecked_into(),
					),
				],
				// Governance account - Snow White
				snow_white.into(),
				// Stakers at genesis
				vec![
					// Bashful
					bashful_sr25519.into(),
					// Doc
					doc_sr25519.into(),
					// Dopey
					dopey_sr25519.into(),
					// Grumpy
					grumpy_sr25519.into(),
					// Happy
					happy_sr25519.into(),
				],
				3,
				EnvironmentConfig {
					stake_manager_address,
					key_manager_address,
					ethereum_chain_id,
					eth_block_safety_margin,
					pending_sign_duration_secs,
					max_ceremony_stage_duration_secs,
					max_extrinsic_retry_attempts,
				},
				eth_init_agg_key,
				ethereum_deployment_block,
				genesis_stake_amount,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Properties
		Some(chainflip_properties()),
		// Extensions
		None,
	))
}

/// Configure initial storage state for FRAME modules.
/// 150 validator limit
fn testnet_genesis(
	wasm_binary: &[u8],
	initial_authorities: Vec<(AccountId, AuraId, GrandpaId)>,
	root_key: AccountId,
	genesis_stakers: Vec<AccountId>,
	min_validators: u32,
	config_set: EnvironmentConfig,
	eth_init_agg_key: [u8; 33],
	ethereum_deployment_block: u64,
	genesis_stake_amount: u128,
) -> GenesisConfig {
	GenesisConfig {
		system: SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
			changes_trie_config: Default::default(),
		},
		validator: ValidatorConfig { blocks_per_epoch: 8 * HOURS },
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| (x.0.clone(), x.0.clone(), session_keys(x.1.clone(), x.2.clone())))
				.collect::<Vec<_>>(),
		},
		flip: FlipConfig { total_issuance: TOTAL_ISSUANCE },
		staking: StakingConfig {
			genesis_stakers: genesis_stakers
				.iter()
				.map(|acct| (acct.clone(), genesis_stake_amount))
				.collect::<Vec<(AccountId, FlipBalance)>>(),
			minimum_stake: MIN_STAKE,
		},
		auction: AuctionConfig {
			validator_size_range: (min_validators, MAX_VALIDATORS),
			winners: initial_authorities
				.iter()
				.map(|(validator_id, ..)| validator_id.clone())
				.collect::<Vec<AccountId>>(),
			minimum_active_bid: genesis_stake_amount,
		},
		aura: AuraConfig { authorities: vec![] },
		grandpa: GrandpaConfig { authorities: vec![] },
		governance: GovernanceConfig { members: vec![root_key], expiry_span: 80000 },
		reputation: ReputationConfig { accrual_ratio: (ACCRUAL_POINTS, ACCRUAL_BLOCKS) },
		environment: config_set,
		vaults: VaultsConfig {
			ethereum_vault_key: eth_init_agg_key.to_vec(),
			ethereum_deployment_block,
		},
		emissions: EmissionsConfig {
			validator_emission_inflation: VALIDATOR_EMISSION_INFLATION_BPS,
			backup_validator_emission_inflation: BACKUP_VALIDATOR_EMISSION_INFLATION_BPS,
		},
	}
}

pub fn chainflip_properties() -> Properties {
	let mut properties = Properties::new();
	// TODO - https://github.com/chainflip-io/chainflip-backend/issues/911
	properties.insert("ss58Format".into(), 42.into());
	properties.insert("tokenDecimals".into(), 18.into());
	properties.insert("tokenSymbol".into(), "FLIP".into());
	properties.insert("color".into(), "#61CFAA".into());

	properties
}
