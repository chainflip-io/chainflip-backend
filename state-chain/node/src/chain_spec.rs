use sp_core::{Pair, Public, sr25519, crypto::UncheckedInto};
use state_chain_runtime::{
	AccountId, AuraConfig, GenesisConfig, GrandpaConfig, FlipConfig, StakingConfig,
	SudoConfig, SystemConfig, WASM_BINARY, Signature, ValidatorConfig, SessionConfig, opaque::SessionKeys,
	FlipBalance
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::traits::{Verify, IdentifyAccount};
use sc_service::ChainType;

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

const TOKEN_ISSUANCE: FlipBalance = 90_000_000;
const TOKEN_FRACTIONS: FlipBalance = 1_000_000_000_000_000_000;
const TOTAL_ISSUANCE: FlipBalance = TOKEN_ISSUANCE * TOKEN_FRACTIONS;

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
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>
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

pub fn development_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Development",
		// ID
		"dev",
		ChainType::Development,
		move || testnet_genesis(
			wasm_binary,
			// Initial PoA authorities
			vec![
				authority_keys_from_seed("Alice"),
			],
			// Sudo account
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			// Pre-funded accounts
			vec![
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			],
			true,
		),
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

pub fn local_testnet_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Local Testnet",
		// ID
		"local_testnet",
		ChainType::Local,
		move || testnet_genesis(
			wasm_binary,
			// Initial PoA authorities
			vec![
				authority_keys_from_seed("Alice"),
				authority_keys_from_seed("Bob"),
			],
			// Sudo account
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			// Pre-funded accounts
			vec![
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Charlie"),
				get_account_id_from_seed::<sr25519::Public>("Dave"),
				get_account_id_from_seed::<sr25519::Public>("Eve"),
				get_account_id_from_seed::<sr25519::Public>("Ferdie"),
				get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
				get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
				get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
				get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
			],
			true,
		),
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

/// Create a local testnet for Chainflip
/// We would have 2 validators at Genesis; Chas and Dave https://en.wikipedia.org/wiki/Chas_%26_Dave
/// Chas would also be Sudo
/// Subsequent validators would need to be added via the session pallet
pub fn chainflip_local_testnet_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Chainflip local Testnet",
		// ID
		"chainflip_local_testnet",
		ChainType::Local,
		move || testnet_genesis(
			wasm_binary,
			// Initial PoA authorities
			vec![
				(
					hex_literal::hex!["b0ca1173a03cc8db1163872255af2a6093dfb124bbb979d8818b0576809cd80a"].into(),
					hex_literal::hex!["b0ca1173a03cc8db1163872255af2a6093dfb124bbb979d8818b0576809cd80a"].unchecked_into(),
					hex_literal::hex!["25582067d09bc511afc4e52d19f1a7fd71acb9ee36c64b8c49a2065a65fd2656"].unchecked_into(),
				),
				(
					hex_literal::hex!["a6ea043d8b3984886f55bd2ffb617ab192ce984b894ff87ca4d7341370b97c6b"].into(),
					hex_literal::hex!["a6ea043d8b3984886f55bd2ffb617ab192ce984b894ff87ca4d7341370b97c6b"].unchecked_into(),
					hex_literal::hex!["07ce8c1a138d10773fb7df3005511550a4c9ded0f6648693f3ca94d0be0bb022"].unchecked_into(),
				),
			],
			// Sudo account
			hex_literal::hex!["b0ca1173a03cc8db1163872255af2a6093dfb124bbb979d8818b0576809cd80a"].into(),
			// Pre-funded accounts
			vec![
				hex_literal::hex!["b0ca1173a03cc8db1163872255af2a6093dfb124bbb979d8818b0576809cd80a"].into(),
				hex_literal::hex!["a6ea043d8b3984886f55bd2ffb617ab192ce984b894ff87ca4d7341370b97c6b"].into(),
			],
			true,
		),
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
/// Configure initial storage state for FRAME modules.
/// 100800 blocks for 7 days at 6 second blocks
/// 150 validator limit
fn testnet_genesis(
	wasm_binary: &[u8],
	initial_authorities: Vec<(AccountId, AuraId, GrandpaId)>,
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
	_enable_println: bool,
) -> GenesisConfig {
	GenesisConfig {
		frame_system: Some(SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
			changes_trie_config: Default::default(),
		}),
		pallet_cf_validator: Some(ValidatorConfig {
			epoch_number_of_blocks: 100800 // 7 days based on 6 second blocks
		}),
		pallet_session: Some(SessionConfig {
			keys: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.0.clone(), session_keys(x.1.clone(), x.2.clone()))
			}).collect::<Vec<_>>(),
		}),
		pallet_cf_flip: Some(FlipConfig {
			total_issuance: TOTAL_ISSUANCE,
		}),
		pallet_cf_staking: Some(StakingConfig {
			genesis_stakers: endowed_accounts.iter()
				.map(|acct| (acct.clone(), TOTAL_ISSUANCE / 100))
				.collect::<Vec<(AccountId, FlipBalance)>>()
		}),
		pallet_aura: Some(AuraConfig {
			authorities: vec![],
		}),
		pallet_grandpa: Some(GrandpaConfig {
			authorities: vec![],
		}),
		pallet_sudo: Some(SudoConfig {
			// Assign network admin rights.
			key: root_key,
		}),
	}
}
