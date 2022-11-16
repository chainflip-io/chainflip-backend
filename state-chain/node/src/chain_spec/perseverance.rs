pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::eth::CHAIN_ID_GOERLI;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("0E1D4594cB44D3E929dc0fb32F1c35A26D6e8e7f"),
	eth_usdc_address: hex_literal::hex!("07865c6E87B9F70255377e024ace6630C1Eaa37F"),
	stake_manager_address: hex_literal::hex!("A599338c8D71ff516854DA954937330aAA25CC44"),
	key_manager_address: hex_literal::hex!("624Ab0aB5334aEAb7853d33503c5553Dfb937499"),
	eth_vault_address: hex_literal::hex!("f2f5D8b18573721361540087A52C05f5FB6d02c1"),
	ethereum_chain_id: CHAIN_ID_GOERLI,
	eth_init_agg_key: hex_literal::hex!(
		"035217961720cf058f447afaebf25e7c14bc44b069ebda50f44dbf25db31b8944c"
	),
	ethereum_deployment_block: 7755959u64,
	genesis_stake_amount: 5_000 * FLIPPERINOS_PER_FLIP,
	min_stake: 10 * FLIPPERINOS_PER_FLIP,
	eth_block_safety_margin: eth::BLOCK_SAFETY_MARGIN as u32,
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

pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;
pub const EXPIRY_SPAN_IN_SECONDS: u64 = 80000;

// Number of online credits required to get `ACCRUAL_REPUTATION_POINTS` of reputation
const ACCRUAL_ONLINE_CREDITS: u32 = 2500;
// Number of reputation points received for having `ACCRUAL_ONLINE_CREDITS`
const ACCRUAL_REPUTATION_POINTS: i32 = 1;
pub const ACCRUAL_RATIO: (i32, u32) = (ACCRUAL_REPUTATION_POINTS, ACCRUAL_ONLINE_CREDITS);

/// Percent of the epoch we are allowed to claim
pub const PERCENT_OF_EPOCH_PERIOD_CLAIMABLE: u8 = 50;
/// Default supply update interval is 24 hours.
pub const SUPPLY_UPDATE_INTERVAL_DEFAULT: u32 = 14_400;
