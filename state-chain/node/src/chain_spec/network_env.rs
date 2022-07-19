use super::*;

/// Environment variables for PARADISE public testnet.
pub const PARADISE: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("B4C15D6Db9EF84699751233a24676B9061d45086"),
	stake_manager_address: hex_literal::hex!("e9B7E052febc1652EB473106ED8425e6111Fa2b1"),
	key_manager_address: hex_literal::hex!("88CB51D36d6D6D8F21203d87331885Cf5C3FEf4f"),
	ethereum_chain_id: 5, // GOERLI
	eth_init_agg_key: hex_literal::hex!(
		"0209d2c7d8e920f234f37f828016ed79397edd2fb046bb1d856d3def64cb796a3c"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 7253755,
	eth_block_safety_margin: 6,
	max_ceremony_stage_duration: 300,
	min_stake: 1_000_000_000_000_000_000_000,
};
