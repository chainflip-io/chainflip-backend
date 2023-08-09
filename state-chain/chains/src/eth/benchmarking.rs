#![cfg(feature = "runtime-benchmarks")]

use super::{
	api::{EthereumReplayProtection, EthereumTransactionBuilder},
	TransactionFee,
};
use crate::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	eth::{
		api::{update_flip_supply::UpdateFlipSupply, EthereumApi},
		to_ethereum_address, Address, AggKey, EthereumTrackedData, SchnorrVerificationComponents,
		Transaction, H256,
	},
	ApiCall,
};
use cf_primitives::EthAmount;
use ethabi::Uint;
use libsecp256k1::{PublicKey, SecretKey};

const SIG_NONCE: [u8; 32] = [1u8; 32];
const PRIVATE_KEY: [u8; 32] = [2u8; 32];

impl BenchmarkValue for SchnorrVerificationComponents {
	fn benchmark_value() -> Self {
		let sig_nonce = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let private_key = SecretKey::parse(&PRIVATE_KEY).expect("Valid private key");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&sig_nonce)).0;

		let agg_key = AggKey::benchmark_value();

		let payload: [u8; 32] =
			EthereumApi::<()>::benchmark_value().threshold_signature_payload().into();
		let signature = agg_key.sign(&payload, &private_key, &sig_nonce);

		Self { s: signature, k_times_g_address }
	}
}

impl BenchmarkValue for Address {
	fn benchmark_value() -> Self {
		to_ethereum_address(PublicKey::from_secret_key(
			&SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce"),
		))
	}
}

impl BenchmarkValueExtended for Address {
	fn benchmark_value_by_id(id: u8) -> Self {
		to_ethereum_address(PublicKey::from_secret_key(
			&SecretKey::parse(&[id; 32]).expect("Valid signature nonce"),
		))
	}
}

impl BenchmarkValue for H256 {
	fn benchmark_value() -> Self {
		EthereumApi::<()>::benchmark_value().threshold_signature_payload()
	}
}

impl BenchmarkValue for AggKey {
	fn benchmark_value() -> Self {
		AggKey::from_private_key_bytes(
			SecretKey::parse(&PRIVATE_KEY).expect("Valid private key").serialize(),
		)
	}
}

impl<E> BenchmarkValue for EthereumApi<E> {
	fn benchmark_value() -> Self {
		EthereumTransactionBuilder::new_unsigned(
			EthereumReplayProtection::default(),
			UpdateFlipSupply::new(1000000u128, 1u64),
		)
		.into()
	}
}

impl BenchmarkValue for Transaction {
	fn benchmark_value() -> Self {
		Transaction {
			chain_id: 31337,
			max_fee_per_gas: Uint::from(1_000_000_000u32).into(),
			gas_limit: Uint::from(21_000u32).into(),
			contract: [0xcf; 20].into(),
			value: 0.into(),
			data: b"do_something()".to_vec(),
			..Default::default()
		}
	}
}

impl BenchmarkValue for EthereumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000, priority_fee: 2_000_000_000 }
	}
}

impl BenchmarkValue for TransactionFee {
	fn benchmark_value() -> Self {
		Self { effective_gas_price: 2_000_000_000, gas_used: 50_000 }
	}
}

impl BenchmarkValue for EthAmount {
	fn benchmark_value() -> Self {
		2000
	}
}
