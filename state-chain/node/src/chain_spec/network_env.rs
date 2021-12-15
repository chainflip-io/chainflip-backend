use super::*;

/// Environment variables for SOUNDCHECK public testnet.
pub const SOUNDCHECK: StateChainEnvironment = StateChainEnvironment {
	stake_manager_address: hex_literal::hex!("42fDb8254192AcE0C01B6f82212C37265a964d06"),
	key_manager_address: hex_literal::hex!("076cf86E7156e50339fB5D34963676f9aBfB99A9"),
	ethereum_chain_id: 4, // RINKEBY
	eth_init_agg_key: hex_literal::hex!(
		"02555a2fcda57ae29bff6fd54f87e78279c76830a28511c3eccef998e96521f4b6"
	),
	genesis_stake_amount: 15_000 * 10u128.pow(18),
	ethereum_deployment_block: 9818850,
};
