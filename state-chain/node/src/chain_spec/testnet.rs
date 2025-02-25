pub use super::common::*;
use super::{get_account_id_from_seed, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, sol::SolAddress};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use cf_utilities::bs58_array;
use sc_service::ChainType;
use sol_prim::consts::{const_address, const_hash};
use sp_core::{sr25519, H256};

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Testnet";
pub const CHAIN_TYPE: ChainType = ChainType::Development;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Development;
pub const PROTOCOL_ID: &str = "flip-test";

// These represent approximately 2 hours on testnet block times
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / (10 * 60);
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 14;
pub const ARBITRUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 * 4;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 * 10 / 4;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	eth_usdc_address: hex_literal::hex!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
	eth_usdt_address: hex_literal::hex!("0DCd1Bf9A1b36cE34237eEaFef220932846BCD82"),
	state_chain_gateway_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	eth_key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	eth_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	arb_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arbusdc_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	eth_address_checker_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_address_checker_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_ARBITRUM_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"02e61afd677cdfbec838c6f309deff0b2c6056f8a27f2c783b68bba6b30f667be6"
	),
	ethereum_deployment_block: 0u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"13d0723c0891a46a0e0931e23fb7c9961c0f87bc73ad965b35cf0f1d84a986b8"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
	sol_genesis_hash: None,
	sol_vault_program: SolAddress(bs58_array("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf")),
	sol_vault_program_data_account: SolAddress(bs58_array(
		"BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG",
	)),
	sol_usdc_token_mint_pubkey: SolAddress(bs58_array(
		"24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p",
	)),
	sol_token_vault_pda_account: SolAddress(bs58_array(
		"7B13iu7bUbBX88eVBqTZkQqrErnTMazPmGLdE5RqdyKZ",
	)),
	sol_usdc_token_vault_ata: SolAddress(bs58_array(
		"9CGLwcPknpYs3atgwtjMX7RhgvBgaqK8wwCvXnmjEoL9",
	)),
	sol_durable_nonces_and_accounts: [
		(
			const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
			const_hash("2qVz58R5aPmF5Q61VaKXnpWQtngdh4Jgbeko32fEcECu"),
		),
		(
			const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
			const_hash("6fxCujPRyyTPzcZWpkDRhvDC4NXf4GB5tCTbQRDnz2iw"),
		),
		(
			const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
			const_hash("2VUnNMpXzohc4284EyuhT7PEuUdqy9E7AfxAP8nrm9cv"),
		),
		(
			const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
			const_hash("7bNgRtjaTgCiXwEagFv2cndco5aTzW1dEXGTZp9EDHgE"),
		),
		(
			const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
			const_hash("DSSpXCVb6LU4a91TAkqyUHGXB1bspfLUxKzb5VhGUvyf"),
		),
		(
			const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
			const_hash("DEwmxJ6xUTXVMnZjvSsRBb9knA1JG3ETT1CQXz5q3yzY"),
		),
		(
			const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
			const_hash("ERJZ2GKvYB2f7MroWy8qEA3xdvMNHg2DUqjBJ6KVzXYE"),
		),
		(
			const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
			const_hash("Ecqa1ZZwjS6Lz74k2kXvVKcrWXJNiVGzLqfe1ftBcCYj"),
		),
		(
			const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
			const_hash("HuksgAnauQ9wTextjMhHVB6oVSCT3GKGb6j1DniSS8eL"),
		),
		(
			const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
			const_hash("2TR5QRLhPzPzB6Gvs4iZsq6Dp8v5w2LamrE9BFrsNkzW"),
		),
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9",
	)),
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 3 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFK7GTahm9qeX5Jjct3yfSvV4qLb8LJaArHL2SL6m9HAzc2sq";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["971b584324592e9977f0ae407eb6b8a1aa5bcd1ca488e54ab49346566f060dd8"];
pub const DOC_ACCOUNT_ID: &str = "cFLxadYLtGwLKA4QZ7XM7KEtmwEohJJy4rVGCG6XK6qS1reye";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["e4c4009bd437cba06a2f25cf02f4efc0cac4525193a88fe1d29196e5d0ff54e8"];
pub const DOPEY_ACCOUNT_ID: &str = "cFNSnvbAqypZTfshHJxx9JLATURCvpQUFexn2bM1TaCZxnpbg";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["5506333c28f3dd39095696362194f69893bc24e3ec553dbff106cdcbfe1beea4"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFNYfLm7YEjWenMB7pBRGMTaawyhYLcRxgrNUqsvZBrKNXvfw";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	vec![
		(
			get_account_id_from_seed::<sr25519::Public>("LP_API"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP API".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_1"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP 1".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_2"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP 2".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_3"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP 3".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_BOOST"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP BOOST".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("BROKER_1"),
			AccountRole::Broker,
			200 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet Broker 1".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("BROKER_2"),
			AccountRole::Broker,
			200 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet Broker 2".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("BROKER_FEE_TEST"),
			AccountRole::Broker,
			200 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet Broker for broker_fee_collection_test".to_vec()),
		),
	]
}

pub const BITCOIN_SAFETY_MARGIN: u64 = 2;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; //todo
