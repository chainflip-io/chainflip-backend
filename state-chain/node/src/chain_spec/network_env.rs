use super::*;

/// Environment variables for PARADISE public testnet.
pub const PARADISE: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Cc36D1fe201d656b6d1733c16B62AD75265dDe9E"),
	stake_manager_address: hex_literal::hex!("4351C1BbF333DeB6589972407aAc9f4cD59609eF"),
	key_manager_address: hex_literal::hex!("cf2975b417fFA6f341e77811248D732A29b498d1"),
	ethereum_chain_id: 5, // GOERLI
	eth_init_agg_key: hex_literal::hex!(
		"032bf2c3d3308f53437c84be18ac3966ded07e50a599c4698dc7ca8dbe8f1116bc"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 7260132,
	eth_block_safety_margin: 6,
	max_ceremony_stage_duration: 300,
	min_stake: 1_000_000_000_000_000_000_000,
};
