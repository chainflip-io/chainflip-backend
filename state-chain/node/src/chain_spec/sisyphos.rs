pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::eth::CHAIN_ID_GOERLI;
use sc_service::ChainType;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Sisyphos";
pub const CHAIN_TYPE: ChainType = ChainType::Live;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("d992EC2354E8A8c12e92372049aa4A7310Bd95eD"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	stake_manager_address: hex_literal::hex!("81B0D1bD77307AFdafCC6A7B9B600C0197eC401f"),
	key_manager_address: hex_literal::hex!("1F51addC19e618E4f8435653AE30Ac473235E59e"),
	eth_vault_address: hex_literal::hex!("75de4859a35A3D3C2296a795Cb3E60bfb9145E0e"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"03670db192abdc18af6cbc42b8c09f3974fd6ba6fa5d2a957c279c91fece270690"
	),
	ethereum_deployment_block: 7826394u64,
	min_stake: MIN_STAKE,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
	max_ceremony_stage_duration: 300,
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
