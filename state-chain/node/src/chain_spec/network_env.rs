use super::*;

/// Environment variables for PARADISE public testnet.
pub const PARADISE: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Aa07486C20F73fF4309495411927E6AE7C884DBa"),
	stake_manager_address: hex_literal::hex!("D4185915BD9533575207DCfdEb6FDeF798B095d3"),
	key_manager_address: hex_literal::hex!("6699A372477f62caA0B0e3465CDA30E789a8F815"),
	ethereum_chain_id: 5, // GOERLI
	eth_init_agg_key: hex_literal::hex!(
		"02071915b34b466951fa08709724a40cc4ad69fbdf3503b372d218654eb0cff592"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 7230032,
	eth_block_safety_margin: 6,
	max_ceremony_stage_duration: 300,
	min_stake: 1_000_000_000_000_000_000_000,
};

/// Environment variables for PARADISE public testnet.
pub const PARADISE: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Aa07486C20F73fF4309495411927E6AE7C884DBa"),
	stake_manager_address: hex_literal::hex!("D4185915BD9533575207DCfdEb6FDeF798B095d3"),
	key_manager_address: hex_literal::hex!("6699A372477f62caA0B0e3465CDA30E789a8F815"),
	ethereum_chain_id: 5, // GOERLI
	eth_init_agg_key: hex_literal::hex!(
		"02071915b34b466951fa08709724a40cc4ad69fbdf3503b372d218654eb0cff592"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 7230032,
	eth_block_safety_margin: 6,
	max_ceremony_stage_duration: 300,
	min_stake: 1_000_000_000_000_000_000_000,
};
