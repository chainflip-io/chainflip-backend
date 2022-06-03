#![cfg_attr(not(feature = "std"), no_std)]
use crate::eth::{
	to_ethereum_address, Address, RawSignedTransaction, SchnorrVerificationComponents, H256,
};

use sp_std::vec;

use crate::benchmarking_value::BenchmarkValue;

use libsecp256k1::{PublicKey, SecretKey};

/// Returns a valid signature for use in benchmarks.
impl BenchmarkValue for SchnorrVerificationComponents {
	fn benchmark_value() -> Self {
		const SIG: [u8; 32] =
			hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
		let k = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		Self { s: SIG, k_times_g_address }
	}
}

impl BenchmarkValue for Address {
	fn benchmark_value() -> Self {
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
		to_ethereum_address(PublicKey::from_secret_key(&SecretKey::parse(&SIG_NONCE).unwrap()))
			.into()
	}
}

impl BenchmarkValue for H256 {
	fn benchmark_value() -> Self {
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
		SIG_NONCE.into()
	}
}

impl BenchmarkValue for RawSignedTransaction {
	fn benchmark_value() -> Self {
		vec![0u8; 32]
	}
}
