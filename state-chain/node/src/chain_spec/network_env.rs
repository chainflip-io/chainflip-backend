use super::*;

/// Environment variables for PARADISE public testnet.
pub const PARADISE: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("9F690f3B12700538f3872AfF504016a546E85c78"),
	stake_manager_address: hex_literal::hex!("41f00272Ac87fFc4c1fd4B3D6E8A932FC423cF80"),
	key_manager_address: hex_literal::hex!("4DBAf1eE163Cfe78F544D00Ce6AEB2bFb590dA75"),
	vault_contract_address: hex_literal::hex!("4DBAf1eE163Cfe78F544D00Ce6AEB2bFb590dA75"),
	ethereum_chain_id: 5, // GOERLI
	eth_init_agg_key: hex_literal::hex!(
		"0273984947f2a25bab820524aca9e71ffda6e928d731ae308e725feddeeb7c5123"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 7260444,
	eth_block_safety_margin: 6,
	max_ceremony_stage_duration: 300,
	min_stake: 1_000_000_000_000_000_000_000,
};
