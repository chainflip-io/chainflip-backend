#![cfg_attr(not(feature = "std"), no_std)]
use crate::{
	benchmarking_value::BenchmarkValue,
	eth::{
		api::{update_flip_supply::UpdateFlipSupply, EthereumApi},
		to_ethereum_address, Address, AggKey, EthereumReplayProtection, RawSignedTransaction,
		SchnorrVerificationComponents, UnsignedTransaction, H256, U256,
	},
	ApiCall,
};
use sp_std::vec;

const SIG_NONCE: [u8; 32] = [1u8; 32];
const PRIVATE_KEY: [u8; 32] = [2u8; 32];

use libsecp256k1::{PublicKey, SecretKey};

impl BenchmarkValue for SchnorrVerificationComponents {
	fn benchmark_value() -> Self {
		let sig_nonce = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let private_key = SecretKey::parse(&PRIVATE_KEY).expect("Valid private key");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&sig_nonce));

		let agg_key = AggKey::benchmark_value();

		let payload: [u8; 32] = EthereumApi::benchmark_value().threshold_signature_payload().into();
		let signature = agg_key.sign(&payload, &private_key, &sig_nonce);

		Self { s: signature, k_times_g_address }
	}
}

impl BenchmarkValue for Address {
	fn benchmark_value() -> Self {
		to_ethereum_address(PublicKey::from_secret_key(
			&SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce"),
		))
		.into()
	}
}

impl BenchmarkValue for H256 {
	fn benchmark_value() -> Self {
		EthereumApi::benchmark_value().threshold_signature_payload().into()
	}
}

impl BenchmarkValue for RawSignedTransaction {
	fn benchmark_value() -> Self {
		vec![
			2, 248, 122, 130, 122, 105, 128, 132, 59, 154, 202, 0, 132, 59, 154, 202, 0, 130, 82,
			8, 148, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
			207, 207, 207, 207, 128, 142, 100, 111, 95, 115, 111, 109, 101, 116, 104, 105, 110,
			103, 40, 41, 192, 128, 160, 183, 150, 224, 39, 109, 137, 176, 224, 38, 52, 210, 240,
			205, 88, 32, 228, 175, 75, 192, 252, 183, 110, 207, 204, 74, 56, 66, 233, 13, 75, 22,
			81, 160, 122, 180, 11, 231, 14, 128, 31, 205, 30, 51, 70, 11, 254, 52, 240, 59, 143,
			57, 9, 17, 101, 141, 73, 229, 139, 3, 86, 167, 123, 148, 50, 192,
		]
		.into()
	}
}

impl BenchmarkValue for AggKey {
	fn benchmark_value() -> Self {
		AggKey::from_private_key_bytes(
			SecretKey::parse(&PRIVATE_KEY).expect("Valid private key").serialize(),
		)
	}
}

impl BenchmarkValue for EthereumApi {
	fn benchmark_value() -> Self {
		EthereumApi::UpdateFlipSupply(UpdateFlipSupply::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
				chain_id: 31337,
				nonce: 15,
			},
			1000000u128,
			1u64,
			&Address::benchmark_value().into(),
		))
	}
}

impl BenchmarkValue for UnsignedTransaction {
	fn benchmark_value() -> Self {
		UnsignedTransaction {
			chain_id: 31337,
			max_fee_per_gas: U256::from(1_000_000_000u32).into(),
			gas_limit: U256::from(21_000u32).into(),
			contract: [0xcf; 20].into(),
			value: 0.into(),
			data: b"do_something()".to_vec(),
			..Default::default()
		}
	}
}
