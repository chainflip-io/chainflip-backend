pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{
	dot::RuntimeVersion,
	sol::{SolAddress, SolHash},
};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use cf_utilities::bs58_array;
use sc_service::ChainType;
use sol_prim::consts::{const_address, const_hash};
use sp_core::H256;
use state_chain_runtime::SetSizeParameters;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Berghain";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Mainnet;
pub const PROTOCOL_ID: &str = "flip-berghain";

// These represent approximately 24 hours on mainnet block times
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 24 * 60 / 10;
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 24 * 3600 / 14;
pub const ARBITRUM_EXPIRY_BLOCKS: u32 = 24 * 3600 * 4;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 24 * 3600 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 24 * 3600 * 10 / 4;
pub const ASSETHUB_EXPIRY_BLOCKS: u32 = 24 * 3600 / 12;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("826180541412D574cf1336d22c0C0a287822678A"),
	eth_usdc_address: hex_literal::hex!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
	eth_usdt_address: hex_literal::hex!("dAC17F958D2ee523a2206206994597C13D831ec7"),
	state_chain_gateway_address: hex_literal::hex!("6995Ab7c4D7F4B03f467Cf4c8E920427d9621DBd"),
	eth_key_manager_address: hex_literal::hex!("cd351d3626Dc244730796A3168D315168eBf08Be"),
	eth_vault_address: hex_literal::hex!("F5e10380213880111522dd0efD3dbb45b9f62Bcc"),
	eth_address_checker_address: hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920"),
	arb_key_manager_address: hex_literal::hex!("BFe612c77C2807Ac5a6A41F84436287578000275"),
	arb_vault_address: hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920"),
	arbusdc_token_address: hex_literal::hex!("af88d065e77c8cC2239327C5EDb3A432268e5831"),
	arb_address_checker_address: hex_literal::hex!("c1B12993f760B654897F0257573202fba13D5481"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_MAINNET,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_MAINNET,
	eth_init_agg_key: hex_literal::hex!(
		"022a1d7efa522ce746bc40a04016178ce38154be1f0537c6957bdeed17057bb955"
	),
	ethereum_deployment_block: 18562942,
	genesis_funding_amount: GENESIS_AUTHORITY_FUNDING,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3" // Polkadot mainnet
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9431, transaction_version: 24 },
	hub_genesis_hash: H256(hex_literal::hex!(
		"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f" // Assethub mainnet
	)),
	hub_vault_account_id: None,
	hub_runtime_version: RuntimeVersion { spec_version: 1003004, transaction_version: 15 },
	sol_genesis_hash: Some(SolHash(bs58_array("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"))),
	sol_vault_program: SolAddress(bs58_array("AusZPVXPoUM8QJJ2SL4KwvRGCQ22cDg6Y4rg7EvFrxi7")),
	sol_vault_program_data_account: SolAddress(bs58_array(
		"ACLMuTFvDAb3oecQQGkTVqpUbhCKHG3EZ9uNXHK1W9ka",
	)),
	sol_usdc_token_mint_pubkey: SolAddress(bs58_array(
		"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
	)),
	sol_token_vault_pda_account: SolAddress(bs58_array(
		"4ZhKJgotJ2tmpYs9Y2NkgJzS7Ac5sghrU4a6cyTLEe7U",
	)),
	sol_usdc_token_vault_ata: SolAddress(bs58_array(
		"8KNqCBB1LKWbtjNxY9v2g1fSBKm2ZRgNNv7rmx2bE6Ce",
	)),
	sol_durable_nonces_and_accounts: [
		(
			const_address("BDKywh4jrvMEFRUkX1bzK8JoyXBY7cmjaZh7bRFpMX4o"),
			const_hash("3pMDqkhnibuv2ARQzjq4K1jn58EvCzC6uF28kiMCUoW2"),
		),
		(
			const_address("2sZp8mnaNZW5FLpbys4rG7RCpVWixmWydRyJmPzNgxi4"),
			const_hash("5rJzAL24yzaqPNE14xFuhdLtLLmUDF3JAfVbZHoBWAUB"),
		),
		(
			const_address("J4Afw1uLrnsQQwEUHQiPe71H3Y3gJQ1oZer5q1QBMViC"),
			const_hash("6ChBdxfZ4ZPLZ7zhVavtjXZrNojg1MdT3Du4VnSnhQ6u"),
		),
		(
			const_address("CCqwvHKHuUSRxxbV7RLSnSYt7XaFrQtFEmaVumLVNmJK"),
			const_hash("6FtBFnt4P25xUnATE6c6XeicKn1ZB6Q5MiGZq4xqqD2D"),
		),
		(
			const_address("3bVqyf58hQHsxbjnqnSkopnoyEHB9v9KQwhZj7h1DucW"),
			const_hash("8UZPPjjKVVjb7TRDx3ZVBnMqrdqYpp6HP2vQVUrxEhn1"),
		),
		(
			const_address("5iKkv5RTvHKzn4VdYLWu48dYsPz5tVniUEa3wHrG9hjB"),
			const_hash("FtLgEitvpnSrcj4adHKcvbYG9SF1C7NLZCk2priDTA6e"),
		),
		(
			const_address("3GGKqshYCGcnQKp6iNh8kb5nbZwtNKSbA9Y7H11eAgyU"),
			const_hash("2xYo9Gv76GGgZs2ikCi8gSgkriEugv5wFhERowyvDx3H"),
		),
		(
			const_address("A2mR1Ytk7R8kGvnRxVLTurZzGr9FwvD8A2ovt3ZRCwQS"),
			const_hash("79NHKfzzZZ4Fmm5mK7D6E16KvwJKWWZpuqyHHiD1xdQ3"),
		),
		(
			const_address("HS6RiBAt9FbC62xJ6kLAH4ekpCW8ZE7HiuHZKNaUbk7a"),
			const_hash("CTyzyX8K9Wwo5zGEZmWxtGpYYwHpGv6YTsFRpi6syLJ4"),
		),
		(
			const_address("14AwUr3FG75E66aaLy7jCbVGaxGCGLdqtpVyBNAFwKac"),
			const_hash("AEzmj9wq8jp7wF46Lrr3Jc2K7xRP58V5Y3cYRVEqtE5J"),
		),
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"J88B7gmadHzTNGiy54c9Ms8BsEXNdB2fntFyhKpk3qoT",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob",
	)),
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

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
	vec![]
}

// Set to zero initially, will be updated by governance to 7% / 1% annual.
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 0;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 0;

pub const SUPPLY_UPDATE_INTERVAL: u32 = 30 * 24 * HOURS;

pub const MIN_FUNDING: FlipBalance = 6 * FLIPPERINOS_PER_FLIP;
pub const GENESIS_AUTHORITY_FUNDING: FlipBalance = 1_000 * FLIPPERINOS_PER_FLIP;
pub const REDEMPTION_TAX: FlipBalance = 5 * FLIPPERINOS_PER_FLIP;

/// Redemption delay on mainnet is 48 HOURS.
/// We add an extra 24 hours buffer.
pub const REDEMPTION_TTL_SECS: u64 = (48 + 24) * 3600;

pub const AUCTION_PARAMETERS: SetSizeParameters =
	SetSizeParameters { min_size: 3, max_size: MAX_AUTHORITIES, max_expansion: MAX_AUTHORITIES };

pub const BITCOIN_SAFETY_MARGIN: u64 = 2;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; // Unused - we use "finalized" instead
