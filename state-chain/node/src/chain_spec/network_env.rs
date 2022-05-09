use super::*;

/// Environment variables for SOUNDCHECK public testnet.
pub const SOUNDCHECK: StateChainEnvironment = StateChainEnvironment {
	stake_manager_address: hex_literal::hex!("3A96a2D552356E17F97e98FF55f69fDFb3545892"),
	key_manager_address: hex_literal::hex!("70d15CD89a551Bcf90fFC72bc006E633c2e4F828"),
	ethereum_chain_id: 4, // RINKEBY
	eth_init_agg_key: hex_literal::hex!(
		"02555a2fcda57ae29bff6fd54f87e78279c76830a28511c3eccef998e96521f4b6"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 9819300,
};

/// Environment variables for SOUNDCHECK TWO public testnet.
pub const SOUNDCHECK_TWO: StateChainEnvironment = StateChainEnvironment {
	stake_manager_address: hex_literal::hex!("168F5e4ba2f13A5EB3fD47754Bba3B49580C14E3"),
	key_manager_address: hex_literal::hex!("3196869D3Fc80cad23e8361ad65D0D9b2119be67"),
	ethereum_chain_id: 4, // RINKEBY
	eth_init_agg_key: hex_literal::hex!(
		"0231ac13900d41cc9a743ce9b1e88c0b5afb4ff370fd161e6534d8472b7052d1ec"
	),
	genesis_stake_amount: 50_000 * 10u128.pow(18),
	ethereum_deployment_block: 10646312,
};
