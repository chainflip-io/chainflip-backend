use super::*;

/// Environment variables for SOUNDCHECK public testnet.
pub const SOUNDCHECK: StateChainEnvironment = StateChainEnvironment {
	stake_manager_address: hex_literal::hex!("6f1031c27017669C75dd8516A167Ec157f692407"),
	key_manager_address: hex_literal::hex!("f7e1F09B84983fcc8479a908546f7Df1c9282b8B"),
	ethereum_chain_id: 4, // RINKEBY
	eth_init_agg_key: hex_literal::hex!(
		"02555a2fcda57ae29bff6fd54f87e78279c76830a28511c3eccef998e96521f4b6"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 9819279,
};
