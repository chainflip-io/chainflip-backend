pub use super::{
	common::*,
	testnet::{BITCOIN_EXPIRY_BLOCKS, ETHEREUM_EXPIRY_BLOCKS, POLKADOT_EXPIRY_BLOCKS},
};
use super::{parse_account, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_MAINNET};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;

// *** Overrides from common
pub const ACCRUAL_RATIO: (i32, u32) = (10, 10);
// ***

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Berghain";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Mainnet;
pub const PROTOCOL_ID: &str = "flip-berghain";

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("826180541412D574cf1336d22c0C0a287822678A"),
	eth_usdc_address: hex_literal::hex!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
	state_chain_gateway_address: hex_literal::hex!("6995Ab7c4D7F4B03f467Cf4c8E920427d9621DBd"),
	key_manager_address: hex_literal::hex!("cd351d3626Dc244730796A3168D315168eBf08Be"),
	eth_vault_address: hex_literal::hex!("F5e10380213880111522dd0efD3dbb45b9f62Bcc"),
	eth_address_checker_address: hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920"),
	ethereum_chain_id: CHAIN_ID_MAINNET,
	eth_init_agg_key: hex_literal::hex!(
		"0250f648bae0db9366550d041e163c9b23b79b1b06be7fac83ba4f338bd02e4024" /* TODO: Albert to
		                                                                      * replace AGG key
		                                                                      * before launch */
	),
	ethereum_deployment_block: 18277081u64, // TODO: Albert to confirm block number before launch
	genesis_funding_amount: 1_000 * FLIPPERINOS_PER_FLIP,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3" // Polkadot mainnet
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9431, transaction_version: 24 }, /* TODO:
	                                                                                      * Dan confirm
	                                                                                      * number */
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

// TODO: change once nodes have been spun up
pub const BASHFUL_ACCOUNT_ID: &str = "cFNzzoURRFHx2fw2EmsCvTc7hBFP34EaP2B23oUcFdbp1FMvx";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["e2e8c8d8a2662d11a96ab6cbf8f627e78d6c77ac011ad0ad65b704976c7c5b6c"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["c2729cfb8507558af71474e9610071585e4ae02c5418e053cdc25106628f9810"];
pub const DOC_ACCOUNT_ID: &str = "cFP2cGErEhxzJfVUxk1gHVuE1ALxHJQx335o19bT7QoSWwjhU";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["e42367696495e88be9b78e7e639bc0a870139bfe43aafb46ea5f934c69903b02"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["5e52d11949673e9ba3a6e3e11c0fc0537bc588de8ac61d41cf04e0ff43dc39a1"];
pub const DOPEY_ACCOUNT_ID: &str = "cFKzr7DwLCRtSkou5H5moKri7g9WwJ4tAbVJv6dZGhLb811Tc";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["5e16d155cf85815a0ba8957762e1e007eec4d5c6fe0b32b4719ca4435c36eb57"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["99cca386ea50fb33d2eee5ebd5574759facb17ddd55241e246b59567f6878242"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFPVXzCyCxKbxJEHhDN1yXrU3VcDPZswHSVHh8HnoGsJsAVYS";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["f8aca257e6ab69e357984a885121c0ee18fcc50185c77966cdaf063df2f89126"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	[vec![
		(
			parse_account("cFHwQ2eJQqRLJWgcHhdgAVCXx2TNRaS3R4Zc98mU2SrkW6AMH"), // TODO: Dan to decide whether we need a Broker API account
			AccountRole::Broker,
			1_000 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Genesis Broker".to_vec()),
		),
		(
			parse_account("cFNaeW7FBpjVxh5haxwmnnATCXriuThVJ8vcyQWKi6SfwWHni"), // TODO: Dan to decide whether we need a Liquidity Provider account
			AccountRole::LiquidityProvider,
			1_000 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Genesis Liquidity Provider".to_vec()),
		),
	]]
	.into_iter()
	.flatten()
	.collect()
}
