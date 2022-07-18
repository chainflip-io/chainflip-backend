use super::*;

/// Environment variables for PARADISE public testnet.
pub const PARADISE: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("eAe3d8CbEFfe236aA8E43697aFc6659522AFf111"),
	stake_manager_address: hex_literal::hex!("7Ee96ee5b3FB01De698CDB572B7f39fFFaDA5Ed8"),
	key_manager_address: hex_literal::hex!("72124fd24dA3CC08fB65BC744D4D3a36C0eE3e51"),
	ethereum_chain_id: 5, // GOERLI
	eth_init_agg_key: hex_literal::hex!(
		"026a87139bd1a893de937b46cda43984499422428ba3edd6d7dc48eed785cb6247"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 7248403,
	eth_block_safety_margin: 6,
	max_ceremony_stage_duration: 300,
	min_stake: 1_000_000_000_000_000_000_000,
};
