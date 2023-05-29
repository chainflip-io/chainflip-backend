pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{btc::BitcoinNetwork, dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, FlipBalance};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Sisyphos";
pub const CHAIN_TYPE: ChainType = ChainType::Live;

pub const BITCOIN_NETWORK: BitcoinNetwork = BitcoinNetwork::Testnet;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("7Bdae61fD4518E200a56E31B6eF16Ec79A3705df"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	state_chain_gateway_address: hex_literal::hex!("1B842f13E2fED40542F8784e4f4bc299e9d5637b"),
	key_manager_address: hex_literal::hex!("62c0226dd6d6361da79B269a7ad9DF8e06BdD1c7"),
	eth_vault_address: hex_literal::hex!("dDA057a42661D0d4D49532aaFf745bB81eF13FD2"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"022d27ebbee85337fe4579cda3be8a15ead924fc19e1bb8cde7898e12dae1e911a"
	),
	ethereum_deployment_block: 9082795u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
	max_ceremony_stage_duration: 300,
	dot_genesis_hash: H256(hex_literal::hex!(
		"1665348821496e14ed56718d4d078e7f85b163bf4e45fa9afbeb220b34ed475a"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9360, transaction_version: 19 },
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

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance)> {
	vec![
		(
			hex_literal::hex!("a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e")
				.into(),
			AccountRole::Broker,
			100 * FLIPPERINOS_PER_FLIP,
		),
		(
			hex_literal::hex!("c0409f949ad2636d34e4c70dd142296fdd4a11323d320aced3d247ad8f9a7902")
				.into(),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
		),
	]
}
