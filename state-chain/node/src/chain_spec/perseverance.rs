pub use super::{
	common::*,
	testnet::{
		BITCOIN_EXPIRY_BLOCKS, ETHEREUM_EXPIRY_BLOCKS, POLKADOT_EXPIRY_BLOCKS, SOLANA_EXPIRY_BLOCKS,
	},
};
use super::{parse_account, SolAddress, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;
pub const PROTOCOL_ID: &str = "flip-pers";

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("0485D65da68b2A6b48C3fA28D7CCAce196798B94"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	state_chain_gateway_address: hex_literal::hex!("38AA40B7b5a70d738baBf6699a45DacdDBBEB3fc"),
	key_manager_address: hex_literal::hex!("Aa4376388C6432d36CFF33198D9f80295482f120"),
	eth_vault_address: hex_literal::hex!("40caFF3f3B6706Da904a7895e0fC7F7922437e9B"),
	eth_address_checker_address: hex_literal::hex!("6Ab555596F452Ba302163d1cBFEFfDFCA7423F07"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"02661d4b647d4b49660976ad402f4890cb8f2f4d872dfa5e1c5f33b1da53f4a637"
	),
	ethereum_deployment_block: 9595582u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"e566d149729892a803c3c4b1e652f09445926234d956a0f166be4d4dea91f536"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
	sol_vault_address: SolAddress([0; 32]), // TODO: fill in the valid Solana address,
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFLbassb4hwQ9iA7dzdVdyumRqkaXnkdYECrThhmrqjFukdVo";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["789523326e5f007f7643f14fa9e6bcfaaff9dd217e7e7a384648a46398245d55"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["7fdaaa9becf88f9f0a3590bd087ddce9f8d284ccf914c542e4c9f0c0e6440a6a"];
pub const DOC_ACCOUNT_ID: &str = "cFLdocJo3bjT7JbT7R46cA89QfvoitrKr9P3TsMcdkVWeeVLa";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["7a467c9e1722b35408618a0cffc87c1e8433798e9c5a79339a10d71ede9e9d79"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["3489d0b548c5de56c1f3bd679dbabe3b0bff44fb5e7a377931c1c54590de5de6"];
pub const DOPEY_ACCOUNT_ID: &str = "cFLdopvNB7LaiBbJoNdNC26e9Gc1FNJKFtvNZjAmXAAVnzCk4";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a4738071f16c71ef3e5d94504d472fdf73228cb6a36e744e0caaf13555c3c01"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["d9a7e774a58c50062caf081a69556736e62eb0c854461f4485f281f60c53160f"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFLsnoJA2YhAGt9815jPqmzK5esKVyhNAwPoeFmD3PEceE12a";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["84f131a66e88e3e5f8dce20d413cab3fbb13769a14a4c7b640b7222863ef353d"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	[
		vec![
			(
				parse_account("cFMTNSQQVfBo2HqtekvhLPfZY764kuJDVFG1EvnnDGYxc3LRW"),
				AccountRole::Broker,
				1_000 * FLIPPERINOS_PER_FLIP,
				Some(b"Chainflip Genesis Broker".to_vec()),
			),
			(
				parse_account("cFN2sr3eDPoyp3G4CAg3EBRMo2VMoYJ7x3rBn51tmXsguYzMX"),
				AccountRole::LiquidityProvider,
				1_000 * FLIPPERINOS_PER_FLIP,
				Some(b"Chainflip Genesis Liquidity Provider".to_vec()),
			),
		],
		phoenix_accounts(),
	]
	.into_iter()
	.flatten()
	.collect()
}

#[ignore = "Only used as a convenience."]
#[test]
fn print_total() {
	let s = phoenix_accounts().iter().map(|(_, _, s, _)| *s).sum::<u128>();
	println!("{s} / {}", s / FLIPPERINOS_PER_FLIP);
}

// Remember to add some extra funds to the dwarves to ensure they remain in the authority set.
fn phoenix_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	include!("perseverance.snapshot")
		.into_iter()
		.map(|(addr, name, balance): (&str, &str, u128)| {
			(
				parse_account(addr),
				AccountRole::Validator,
				balance,
				if name.is_empty() { None } else { Some(name.as_bytes().to_vec()) },
			)
		})
		.collect::<Vec<_>>()
}

pub const BITCOIN_SAFETY_MARGIN: u64 = 5;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
