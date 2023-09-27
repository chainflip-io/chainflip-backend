pub use super::common::*;
use super::{parse_account, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_MAINNET};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;

// *** Overrides from common
pub const ACCRUAL_RATIO: (i32, u32) = (10, 10);
// ***

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-KitKat";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Mainnet;
pub const PROTOCOL_ID: &str = "flip-kitkat";

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("6fA66Cb44dE97CaA1aE7742019DF6e7AFA3F9b51"),
	eth_usdc_address: hex_literal::hex!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
	state_chain_gateway_address: hex_literal::hex!("B5E2C8fa04Ae426167490a0437c4F20556c9B07C"),
	key_manager_address: hex_literal::hex!("697C9BCA7916305C1D9377AB0E985da1411Dd70b"),
	eth_vault_address: hex_literal::hex!("806cDBA7E42AdDE2B6cb09b748943E4B9A9188E5"),
	eth_address_checker_address: hex_literal::hex!("D2D873BCaE693C9Cb9F7757183012edD671d2216"),
	ethereum_chain_id: CHAIN_ID_MAINNET,
	eth_init_agg_key: hex_literal::hex!(
		"0250f648bae0db9366550d041e163c9b23b79b1b06be7fac83ba4f338bd02e4024"
	),
	ethereum_deployment_block: 18227257u64,
	genesis_funding_amount: 1_000 * FLIPPERINOS_PER_FLIP,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3" // Polkadot mainnet
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9430, transaction_version: 24 },
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFNBWrx4Wx68hVugPVX3KAtKwbsmHjw6ozAXE3d663bwtYF5R";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["beb23228343fc71c913a10299f577cbc20ee0eb44dcf8a698ab861c76223495b"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["92e620fa4cc3736bbef778b5852309936c0ce640ad50c1f4f36fc15eba7f4ed8"];
pub const DOC_ACCOUNT_ID: &str = "cFJHNnou7QwegcEEhL3xeD1aUrjeaL5Ydqtrfyd7ousKoiLPU";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["123990561086190def905deab1f5c3fe1f7dd08585e677ff4fe3196e1201a82e"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["4a11427487645aade5f5134904668a9dcde93e493f668f0b347f23d3cd4d7c76"];
pub const DOPEY_ACCOUNT_ID: &str = "cFJyZdYw1p9bkYNUuwDppkfKrGinn8r4ZWaPvSaxSvrpiSTa2";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["30dfdf38642c300105e2ac604b92d08f1e26f01459539bfd7f8c51cf60e0ce68"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["39df8c764fb991819aeb94bfb2e7809a2728660113e6ec0758e751d3c00f4fcd"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFKsNoWaviRPS5s4xPxHxEXWBwAm3Q4JGXoXY3HXcsfWgun1D";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["58642e85c7889f9cda6e5d87249c6ab4c6d9b2f6bad8c5986cab81c6317d4e61"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	[vec![
		(
			parse_account("cFHwQ2eJQqRLJWgcHhdgAVCXx2TNRaS3R4Zc98mU2SrkW6AMH"),
			AccountRole::Broker,
			1_000 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Genesis Broker".to_vec()),
		),
		(
			parse_account("cFNaeW7FBpjVxh5haxwmnnATCXriuThVJ8vcyQWKi6SfwWHni"),
			AccountRole::LiquidityProvider,
			1_000 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Genesis Liquidity Provider".to_vec()),
		),
	]]
	.into_iter()
	.flatten()
	.collect()
}
