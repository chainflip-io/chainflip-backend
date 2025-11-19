use super::*;
use ethereum_eip712::{build_eip712_data::build_eip712_typed_data, eip712::Eip712};
use serde_json;
use sp_core::U256;
use state_chain_runtime::RuntimeCall;

#[test]
fn test_process_deposits_call() {
	use cf_chains::sol::VaultSwapOrDepositChannelId;
	use pallet_cf_ingress_egress::DepositWitness;
	let hash = test_build_eip712_typed_data(RuntimeCall::SolanaIngressEgress(
		pallet_cf_ingress_egress::Call::process_deposits {
			deposit_witnesses: vec![
				DepositWitness {
					deposit_address: [3u8; 32].into(),
					amount: 5000u64,
					asset: cf_chains::assets::sol::Asset::Sol,
					deposit_details: VaultSwapOrDepositChannelId::Channel(Default::default()),
				},
				DepositWitness {
					deposit_address: [4u8; 32].into(),
					amount: 6000u64,
					asset: cf_chains::assets::sol::Asset::SolUsdc,
					deposit_details: VaultSwapOrDepositChannelId::Channel(Default::default()),
				},
			],
			block_height: 6u64,
		},
	));

	assert_eq!(hash, "ee8c4e6d9fd7b7bb167f1703b32875e24ce29be859f123c5c95c6b97a8ef06cf");
}

#[test]
fn test_swap_request_call() {
	let hash = test_build_eip712_typed_data(RuntimeCall::LiquidityProvider(pallet_cf_lp::Call::schedule_swap {
			amount: 12345u128,
			input_asset: cf_primitives::Asset::Sol,
			output_asset: cf_primitives::Asset::Btc,
			retry_duration: 543u32,
			price_limits: cf_primitives::PriceLimits {
				min_price: U256::from_dec_str("115792089237316195423570985008687907853269984665640564039457584007913129639935").unwrap(),
				max_oracle_price_slippage: Some(98u16),
			},
			dca_params: None,
		}));

	assert_eq!(hash, "43ad4628e1b07ca4887cb8cad3995898a744e68f4ee5d3f6488d3596e6186c3b");
}

fn test_build_eip712_typed_data(call: RuntimeCall) -> String {
	let chainflip_network = cf_primitives::ChainflipNetwork::Mainnet;

	let transaction_metadata = TransactionMetadata { nonce: 1, expiry_block: 1000 };
	let spec_version = 1;

	let typed_data_result = build_eip712_typed_data(
		pallet_cf_environment::submit_runtime_call::ChainflipExtrinsic {
			call: call.clone(),
			transaction_metadata,
		},
		chainflip_network.as_str().to_string(),
		spec_version,
	)
	.unwrap();

	println!(
		"Typed Data: {:#?}",
		serde_json::to_writer_pretty(std::io::stdout(), &typed_data_result).unwrap()
	);

	let domain = ethereum_eip712::eip712::EIP712Domain {
		name: Some(chainflip_network.as_str().to_string()),
		version: Some(spec_version.to_string()),
		chain_id: None,
		verifying_contract: None,
		salt: None,
	};

	hex::encode(ethereum_eip712::hash::keccak256(
		ethereum_eip712::encode_eip712_using_type_info(
			pallet_cf_environment::submit_runtime_call::ChainflipExtrinsic {
				call,
				transaction_metadata,
			},
			domain,
		)
		.unwrap()
		.encode_eip712()
		.unwrap(),
	))
}
