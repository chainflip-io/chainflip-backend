pub use super::{
	common::*,
	testnet::{
		ARBITRUM_EXPIRY_BLOCKS, BITCOIN_EXPIRY_BLOCKS, ETHEREUM_EXPIRY_BLOCKS,
		POLKADOT_EXPIRY_BLOCKS,
	},
};
use super::{parse_account, StateChainEnvironment};
use cf_chains::dot::RuntimeVersion;
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;
pub const PROTOCOL_ID: &str = "flip-pers-2";

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 1_000 * FLIPPERINOS_PER_FLIP;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("dC27c60956cB065D19F08bb69a707E37b36d8086"),
	eth_usdc_address: hex_literal::hex!("5fd84259d66Cd46123540766Be93DFE6D43130D7"),
	state_chain_gateway_address: hex_literal::hex!("A34a967197Ee90BB7fb28e928388a573c5CFd099"),
	eth_key_manager_address: hex_literal::hex!("4981b1329F29E720642266fc6e172C3f78159dff"),
	eth_vault_address: hex_literal::hex!("36eaD71325604DC15d35FAE584D7b50646D81753"),
	eth_address_checker_address: hex_literal::hex!("58eaCD5A40EEbCbBCb660f178F9A46B1Ad63F846"),
	arb_key_manager_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	arb_vault_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	arbusdc_token_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	arb_address_checker_address: hex_literal::hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), /* put correct values here */
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"021cf3c105fbc7112f3394c3e176463ec59600f1e7005ad8d68f66840264998667"
	),
	ethereum_deployment_block: 5429883u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"e566d149729892a803c3c4b1e652f09445926234d956a0f166be4d4dea91f536"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
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
	[vec![
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
	]]
	.into_iter()
	.flatten()
	.collect()
}

pub const BITCOIN_SAFETY_MARGIN: u64 = 5;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
