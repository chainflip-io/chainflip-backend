pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Sisyphos";
pub const CHAIN_TYPE: ChainType = ChainType::Live;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("8437f6b8BCACb632cC4dD627bA3a8E6E3326A418"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	stake_manager_address: hex_literal::hex!("d81663aeC346006d890b8C9182dC354BE9663F19"),
	key_manager_address: hex_literal::hex!("177c941BA853e731c66758675628B4Dc64Aa186A"),
	eth_vault_address: hex_literal::hex!("83cB2d501E90792Ee3D5e049F43805126a7684c6"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"0351267cb549f545f03322391351c2e101673db664800baa433e20ba90972ec616"
	),
	ethereum_deployment_block: 8304200u64,
	genesis_stake_amount: GENESIS_STAKE_AMOUNT,
	min_stake: MIN_STAKE,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
	max_ceremony_stage_duration: 300,
	dot_genesis_hash: H256(hex_literal::hex!(
		"1665348821496e14ed56718d4d078e7f85b163bf4e45fa9afbeb220b34ed475a"
	)),
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
	hex_literal::hex!["84f134a4cc6bf41d3239bbe097eac4c8f83e78b468e6c49ed5cd2ddc51a07a29"];
