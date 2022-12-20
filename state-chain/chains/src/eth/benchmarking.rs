use crate::{
	benchmarking_value::BenchmarkValue,
	eth::{
		api::{update_flip_supply::UpdateFlipSupply, EthereumApi},
		to_ethereum_address, Address, AggKey, EthereumReplayProtection,
		SchnorrVerificationComponents, TrackedData, Transaction, H256, U256,
	},
	ApiCall, Vec,
};

const SIG_NONCE: [u8; 32] = [1u8; 32];
const PRIVATE_KEY: [u8; 32] = [2u8; 32];

use cf_primitives::EthAmount;
use libsecp256k1::{PublicKey, SecretKey};

use super::{Ethereum, TransactionFee};

impl BenchmarkValue for SchnorrVerificationComponents {
	fn benchmark_value() -> Self {
		let sig_nonce = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let private_key = SecretKey::parse(&PRIVATE_KEY).expect("Valid private key");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&sig_nonce));

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
		.into()
	}
}

impl BenchmarkValue for H256 {
	fn benchmark_value() -> Self {
		EthereumApi::<()>::benchmark_value().threshold_signature_payload()
	}
}

impl BenchmarkValue for Vec<u8> {
	fn benchmark_value() -> Self {
		hex_literal::hex!("02f87a827a6980843b9aca00843b9aca0082520894cfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf808e646f5f736f6d657468696e672829c080a0b796e0276d89b0e02634d2f0cd5820e4af4bc0fcb76ecfcc4a3842e90d4b1651a07ab40be70e801fcd1e33460bfe34f03b8f390911658d49e58b0356a77b9432c0").to_vec()
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

impl BenchmarkValue for Transaction {
	fn benchmark_value() -> Self {
		Transaction {
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

impl BenchmarkValue for TrackedData<Ethereum> {
	fn benchmark_value() -> Self {
		Self { block_height: 1000, base_fee: 10_000_000_000, priority_fee: 2_000_000_000 }
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
