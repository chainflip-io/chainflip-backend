pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{dot::RuntimeVersion, eth::CHAIN_ID_GOERLI};
use sc_service::ChainType;
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Testnet";
pub const CHAIN_TYPE: ChainType = ChainType::Local;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	eth_usdc_address: hex_literal::hex!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
	stake_manager_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	eth_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"02e61afd677cdfbec838c6f309deff0b2c6056f8a27f2c783b68bba6b30f667be6"
	),
	ethereum_deployment_block: 0u64,
	genesis_stake_amount: GENESIS_STAKE_AMOUNT,
	min_stake: MIN_STAKE,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
	max_ceremony_stage_duration: 300,
	dot_genesis_hash: H256(hex_literal::hex!(
		"1eb7cacaa47d19edbd35c578523b2278f9b4d49a4a25318f3fe3a21bf30d769a"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9320, transaction_version: 16 },
};

pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["971b584324592e9977f0ae407eb6b8a1aa5bcd1ca488e54ab49346566f060dd8"];
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["e4c4009bd437cba06a2f25cf02f4efc0cac4525193a88fe1d29196e5d0ff54e8"];
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["5506333c28f3dd39095696362194f69893bc24e3ec553dbff106cdcbfe1beea4"];
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"];
