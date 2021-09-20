use sc_service::{ChainType, Properties};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::UncheckedInto, sr25519, Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::traits::{IdentifyAccount, Verify};
use state_chain_runtime::{
	opaque::SessionKeys, AccountId, AuctionConfig, AuraConfig, EmissionsConfig, FlipBalance,
	FlipConfig, GenesisConfig, GovernanceConfig, GrandpaConfig, ReputationConfig, SessionConfig,
	Signature, StakingConfig, SystemConfig, ValidatorConfig, DAYS, WASM_BINARY,
};

const TOTAL_ISSUANCE: FlipBalance = {
	const TOKEN_ISSUANCE: FlipBalance = 90_000_000;
	const TOKEN_DECIMALS: u32 = 18;
	const TOKEN_FRACTIONS: FlipBalance = 10u128.pow(TOKEN_DECIMALS);
	TOKEN_ISSUANCE * TOKEN_FRACTIONS
};

const MAX_VALIDATORS: u32 = 150;

const BLOCK_EMISSIONS: FlipBalance = {
	const ANNUAL_INFLATION_PERCENT: FlipBalance = 10;
	const ANNUAL_INFLATION: FlipBalance = TOTAL_ISSUANCE * ANNUAL_INFLATION_PERCENT / 100;
	// Note: DAYS is the number of blocks in a day.
	ANNUAL_INFLATION / 365 / DAYS as u128
};

// Number of blocks to be online to accrue a point
pub const ACCRUAL_BLOCKS: u32 = 2500;
// Number of accrual points
pub const ACCRUAL_POINTS: i32 = 1;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig>;

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

/// Generate an Aura authority key.
pub fn authority_keys_from_seed(s: &str) -> (AccountId, AuraId, GrandpaId) {
	(
		get_account_id_from_seed::<sr25519::Public>(s),
		get_from_seed::<AuraId>(s),
		get_from_seed::<GrandpaId>(s),
	)
}

/// Start a single node development chain
/// 300 block epochs = 30 mins at 6 second block time
pub fn development_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		"Develop",
		"dev",
		ChainType::Development,
		move || {
			testnet_genesis(
				wasm_binary,
				// Initial PoA authorities
				vec![authority_keys_from_seed("Alice")],
				// Sudo account
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				// Pre-funded accounts
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
				1,
				300,
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
/// 100 block epochs = 10 mins at 6 second block time
pub fn cf_development_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;
	let bashful_sr25519 =
		hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
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
				// Sudo account - Bashful
				bashful_sr25519.into(),
				// Pre-funded accounts
				vec![
					// Bashful
					bashful_sr25519.into(),
				],
				1,
				100,
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

/// Initialise a Chainflip testnet
/// 300 block epochs = 30 mins at 6 second block time
pub fn chainflip_testnet_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;
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
	Ok(ChainSpec::from_genesis(
		"Internal testnet",
		"test",
		ChainType::Live,
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
				// Sudo account - Bashful
				bashful_sr25519.into(),
				// Pre-funded accounts
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
				300,
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
	_root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
	min_validators: u32,
	epoch_number_of_blocks: u32,
) -> GenesisConfig {
	GenesisConfig {
		frame_system: Some(SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
			changes_trie_config: Default::default(),
		}),
		pallet_cf_validator: Some(ValidatorConfig {
			epoch_number_of_blocks,
		}),
		pallet_session: Some(SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						session_keys(x.1.clone(), x.2.clone()),
					)
				})
				.collect::<Vec<_>>(),
		}),
		pallet_cf_flip: Some(FlipConfig {
			total_issuance: TOTAL_ISSUANCE,
		}),
		pallet_cf_staking: Some(StakingConfig {
			genesis_stakers: endowed_accounts
				.iter()
				.map(|acct| (acct.clone(), TOTAL_ISSUANCE / 100))
				.collect::<Vec<(AccountId, FlipBalance)>>(),
		}),
		pallet_cf_auction: Some(AuctionConfig {
			auction_size_range: (min_validators, MAX_VALIDATORS),
		}),
		pallet_aura: Some(AuraConfig {
			authorities: vec![],
		}),
		pallet_grandpa: Some(GrandpaConfig {
			authorities: vec![],
		}),
		pallet_cf_emissions: Some(EmissionsConfig {
			emission_per_block: BLOCK_EMISSIONS,
			..Default::default()
		}),
		pallet_cf_governance: Some(GovernanceConfig {
			members: vec![
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Charlie"),
				get_account_id_from_seed::<sr25519::Public>("Dave"),
			],
			expiry_span: 80000,
		}),
		pallet_cf_reputation: Some(ReputationConfig {
			accrual_ratio: (ACCRUAL_POINTS, ACCRUAL_BLOCKS),
		}),
	}
}

pub fn chainflip_properties() -> Properties {
	let mut properties = Properties::new();

	properties.insert("ss58Format".into(), 28.into());
	properties.insert("tokenDecimals".into(), 18.into());
	properties.insert("tokenSymbol".into(), "FLIP".into());
	properties.insert("color".into(), "#61CFAA".into());

	properties
}
