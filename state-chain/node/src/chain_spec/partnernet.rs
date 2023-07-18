pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{btc::BitcoinNetwork, dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Partnernet";
pub const CHAIN_TYPE: ChainType = ChainType::Live;

pub const BITCOIN_NETWORK: BitcoinNetwork = BitcoinNetwork::Testnet;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("1Ea4F05a319A8f779F05E153974605756bB13D4F"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	state_chain_gateway_address: hex_literal::hex!("07B3Bef16c640B072085BF83C24b6C43000aE056"),
	key_manager_address: hex_literal::hex!("925B762bfDE25b9673f672E30aeBE2051177f5Cd"),
	eth_vault_address: hex_literal::hex!("AfD0C34E6d25F707d931F8b7EE9cf0Ff52160A46"),
	eth_address_checker_address: hex_literal::hex!("aeB5C0Df4826162e48b1ec54D9445B935B0F05D0"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"021a4bea2382419792ac874cb18d8f3069517afa263a3bf812eac18108f17d33cb"
	),
	ethereum_deployment_block: 9365258u64,
	genesis_funding_amount: 1_000 * FLIPPERINOS_PER_FLIP,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"bb5111c1747c9e9774c2e6bd229806fb4d7497af2829782f39b977724e490b5c"
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
