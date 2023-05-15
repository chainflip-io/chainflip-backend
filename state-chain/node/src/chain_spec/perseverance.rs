pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, FlipBalance};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";
pub const CHAIN_TYPE: ChainType = ChainType::Live;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("0E1D4594cB44D3E929dc0fb32F1c35A26D6e8e7f"),
	eth_usdc_address: hex_literal::hex!("07865c6E87B9F70255377e024ace6630C1Eaa37F"),
	state_chain_gateway_address: hex_literal::hex!("A599338c8D71ff516854DA954937330aAA25CC44"),
	key_manager_address: hex_literal::hex!("624Ab0aB5334aEAb7853d33503c5553Dfb937499"),
	eth_vault_address: hex_literal::hex!("f2f5D8b18573721361540087A52C05f5FB6d02c1"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"035217961720cf058f447afaebf25e7c14bc44b069ebda50f44dbf25db31b8944c"
	),
	ethereum_deployment_block: 8304200u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
	max_ceremony_stage_duration: 300,
	// TODO: update this to the correct value for perseverance
	dot_genesis_hash: H256([0xcf; 32]),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9320, transaction_version: 16 },
};

pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["789522255805797fd542969100ab7689453cd5697bb33619f5061e47b7c1564f"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["e4f9260f8ed3bd978712e638c86f85a57f73f9aadd71538eea52f05dab0df2dd"];
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["7a46817c60dff154901510e028f865300452a8d7a528f573398313287c689929"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["15bb6ba6d89ee9fac063dbf5712a4f53fa5b5a7b18e805308575f4732cb0061f"];
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a47312f9bd71d480b1e8f927fe8958af5f6345ac55cb89ef87cff5befcb0949"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["7c937c229aa95b19732a4a2e306a8cefb480e7c671de8fc416ec01bb3eedb749"];
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["84f131a66e88e3e5f8dce20d413cab3fbb13769a14a4c7b640b7222863ef353d"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance)> {
	vec![(
		hex_literal::hex!("b81ba08bd1225107cecf9cb1ca094292b6e4eafbedd048938fb020017171cc1f")
			.into(),
		AccountRole::Broker,
		100 * FLIPPERINOS_PER_FLIP,
	)]
}
