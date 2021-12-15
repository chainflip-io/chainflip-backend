use super::*;

/// Environment variables for SOUNDCHECK public testnet.
///
pub const SOUNDCHECK: StateChainEnvironment = StateChainEnvironment {
	stake_manager_address: hex_literal::hex!("9Dfaa29bEc7d22ee01D533Ebe8faA2be5799C77F"),
	key_manager_address: hex_literal::hex!("36fB9E46D6cBC14600D9089FD7Ce95bCf664179f"),
	ethereum_chain_id: 4, // RINKEBY
	eth_init_agg_key: hex_literal::hex!("02e61afd677cdfbec838c6f309deff0b2c6056f8a27f2c783b68bba6b30f667be6"),
	genesis_stake_amount: 10_000 * 10u128.pow(18),
};
