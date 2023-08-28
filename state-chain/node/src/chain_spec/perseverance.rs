pub use super::common::*;
use super::{parse_account, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;

// *** Overrides from common
pub const ACCRUAL_RATIO: (i32, u32) = (10, 10);
// ***

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;
pub const PROTOCOL_ID: &str = "flip-pers";

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("1194C91d47Fc1b65bE18db38380B5344682b67db"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	state_chain_gateway_address: hex_literal::hex!("C960C4eEe4ADf40d24374D85094f3219cf2DD8EB"),
	key_manager_address: hex_literal::hex!("56a10b82180D4b8F6203541FEaF2c88a3999e847"),
	eth_vault_address: hex_literal::hex!("F1B061aCCDAa4B7c029128b49aBc047F89D5CB8d"),
	eth_address_checker_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"036e4e4d5e5b36c66ad380541929a66bb1f7eaa267b3fa07b342ef390f9a271093"
	),
	ethereum_deployment_block: 9216168u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"bb5111c1747c9e9774c2e6bd229806fb4d7497af2829782f39b977724e490b5c"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9360, transaction_version: 19 },
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
	[
		include!("perseverance.snapshot")
	]
	.into_iter()
	.map(|(addr, name, balance)| {
		(
			parse_account(addr),
			AccountRole::Validator,
			balance,
			if name.is_empty() { None } else { Some(name.as_bytes().to_vec()) },
		)
	})
	.collect::<Vec<_>>()
}
