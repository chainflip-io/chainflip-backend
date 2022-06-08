#![cfg_attr(not(feature = "std"), no_std)]
use crate::eth::{
	to_ethereum_address, Address, AggKey, RawSignedTransaction, SchnorrVerificationComponents, H256,
};

use crate::eth::{api::EthereumApi, EthereumReplayProtection};

use crate::eth::api::update_flip_supply::UpdateFlipSupply;

use crate::ApiCall;

use crate::benchmarking_value::BenchmarkValue;

const K: [u8; 32] = [1u8; 32];

use libsecp256k1::{PublicKey, SecretKey};

/// Returns a valid signature for use in benchmarks.
impl BenchmarkValue for SchnorrVerificationComponents {
	fn benchmark_value() -> Self {
		// ApiCall::benchmark_value().threshold_signature_payload();
		// AggKey::benchmark_value().sign()
		let k = SecretKey::parse(&K).expect("Valid signature nonce");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));

		let secret_key = AggKey::from_private_key_bytes(k.serialize());

		let payload: [u8; 32] = EthereumApi::benchmark_value().threshold_signature_payload().into();
		let signature = secret_key.sign(&payload, &k, &k);

		Self { s: signature, k_times_g_address }
	}
}

impl BenchmarkValue for Address {
	fn benchmark_value() -> Self {
		let k = SecretKey::parse(&K).expect("Valid signature nonce");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		k_times_g_address.into()
	}
}

impl BenchmarkValue for H256 {
	fn benchmark_value() -> Self {
		EthereumApi::benchmark_value().threshold_signature_payload().into()
	}
}

impl BenchmarkValue for RawSignedTransaction {
	fn benchmark_value() -> Self {
		SchnorrVerificationComponents::benchmark_value().s.to_vec()
	}
}

impl BenchmarkValue for AggKey {
	fn benchmark_value() -> Self {
		let k = SecretKey::parse(&K).expect("Valid signature nonce");
		AggKey::from_private_key_bytes(k.serialize())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for EthereumApi {
	#[cfg(feature = "runtime-benchmarks")]
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
