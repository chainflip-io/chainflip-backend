use cf_chains::eth::CHAIN_ID_GOERLI;
use state_chain_runtime::constants::common::FLIPPERINOS_PER_FLIP;

use super::StateChainEnvironment;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("8e71CEe1679bceFE1D426C7f23EAdE9d68e62650"),
	eth_usdc_address: hex_literal::hex!("07865c6e87b9f70255377e024ace6630c1eaa37f"),
	stake_manager_address: hex_literal::hex!("ff99F65D0042393079442f68F47C7AE984C3F930"),
	key_manager_address: hex_literal::hex!("d654BBBd3416C65e9B9Cf8E6618907679Ef840A9"),
	eth_vault_address: hex_literal::hex!("77a8c6dF73117E72548a1E63e0Bf15D29D283ceE"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"026015128a0b2e09b4a241e4e14ece67bfd0dad1c978b10d317785d82046c1f9b2"
	),
	ethereum_deployment_block: 7909675u64,
	genesis_stake_amount: 5_000 * FLIPPERINOS_PER_FLIP,
	min_stake: 10 * FLIPPERINOS_PER_FLIP,
	eth_block_safety_margin: 4,
	max_ceremony_stage_duration: 300,
};

pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["789523326e5f007f7643f14fa9e6bcfaaff9dd217e7e7a384648a46398245d55"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["7fdaaa9becf88f9f0a3590bd087ddce9f8d284ccf914c542e4c9f0c0e6440a6a"];
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["7a467c9e1722b35408618a0cffc87c1e8433798e9c5a79339a10d71ede9e9d79"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["3489d0b548c5de56c1f3bd679dbabe3b0bff44fb5e7a377931c1c54590de5de6"];
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a4738071f16c71ef3e5d94504d472fdf73228cb6a36e744e0caaf13555c3c01"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["d9a7e774a58c50062caf081a69556736e62eb0c854461f4485f281f60c53160f"];
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["84f131a66e88e3e5f8dce20d413cab3fbb13769a14a4c7b640b7222863ef353d"];
