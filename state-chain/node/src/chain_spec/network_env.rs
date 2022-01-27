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
	eth_block_safety_margin: 4,
	pending_sign_duration_secs: 500,
	max_ceremony_stage_duration_secs: 300,
	max_extrinsic_retry_attempts: 10,
};
