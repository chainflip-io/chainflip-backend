pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{arb::CHAIN_ID_MAINNET, dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Partnernet";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("8437f6b8BCACb632cC4dD627bA3a8E6E3326A418"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	state_chain_gateway_address: hex_literal::hex!("d81663aeC346006d890b8C9182dC354BE9663F19"),
	key_manager_address: hex_literal::hex!("177c941BA853e731c66758675628B4Dc64Aa186A"),
	eth_vault_address: hex_literal::hex!("83cB2d501E90792Ee3D5e049F43805126a7684c6"),
	arb_key_manager_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	arb_vault_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	arbusdc_token_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	eth_address_checker_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_address_checker_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	ethereum_chain_id: CHAIN_ID_GOERLI,
	arbitrum_chain_id: CHAIN_ID_MAINNET, // put the correct chain id for arb testnet
	eth_init_agg_key: hex_literal::hex!(
		"0351267cb549f545f03322391351c2e101673db664800baa433e20ba90972ec616"
	),
	ethereum_deployment_block: 8304200u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"1665348821496e14ed56718d4d078e7f85b163bf4e45fa9afbeb220b34ed475a"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9360, transaction_version: 19 },
};

pub const BASHFUL_ACCOUNT_ID: &str = "cFJbas2CnAx1ettAK16fhZ7Axqv6DjJK8jYpmFGdiTQD1vsmN";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["201c9f049fef9086d497ac00f63e3195130756de690e1a3a5637378c02a02332"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["c4a9318659121c5725f34937e22fb59a5a862803bb36af5237d16565d85f9acd"];
pub const DOC_ACCOUNT_ID: &str = "cFMdoc1XzvE8BPYz2A4rCMcBKx3UTtDzfo5x9eV8yTQwdyHgF";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["a682b3f5c73c21f5a1441da641c7c31b93bd7e42608be4c6ba197094c484025f"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["4bfba9f793beee0e32139c6eb434f7c85622644eecd89e4788346fac1f771775"];
pub const DOPEY_ACCOUNT_ID: &str = "cFLdopcmQCjzkWvBa2EBoHMAS3VTcmkPJtFC7JHQLLPgp72LK";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a473384eda5c57c42f9cc0e3bad834b824471c54bda2e2f528fa76018cbda03"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["2cc0863926c4f36d530ab1141b30f7a0518f83807fd12517fed4becc28491824"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFHsnoKTTAhACV8Z792YMkx1yYUkxyhy8wFpt9Q1nikGDSPNu";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["003c7eb0fdbab1061c8b0962aac41726839ba7922e1b36fddc4e8b55c81efc24"];

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 168 * HOURS;

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	vec![
		(
			hex_literal::hex!("90b6b073a06f73475704c33c61a0dce23dc094cf91f959011be8d374a9672d50")
				.into(),
			AccountRole::Broker,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Partnernet Broker".to_vec()),
		),
		(
			hex_literal::hex!("8a85ab1ce533e2f51987a54774826e1ce6605dfa72ac61987f1286e3f765ea3c")
				.into(),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Partnernet LP".to_vec()),
		),
	]
}
